import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('../api', () => ({
  api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }),
  win: new Proxy({}, { get: () => async () => () => {} })
}));

import { setLang } from '../i18n.svelte';
import TitleBar from '../layout/TitleBar.svelte';
import UpdateButton from '../layout/UpdateButton.svelte';
import { W1, makeApi, makeUpdate } from '../mock-api';
import { store } from '../store.svelte';
import { toasts } from '../ui/toast.svelte';
import Toasts from '../ui/Toasts.svelte';
import { DEFAULT_PREFS, updates } from '../update.svelte';
import UpdateDialog from './UpdateDialog.svelte';
import UpdateSettings from './UpdateSettings.svelte';

const player = { name: 'amy', ip: '1.1.1.1', id: 'x', joinedAt: 1 };
const clip = { writeText: vi.fn(async () => {}) };

function reset() {
  mock.api = makeApi({ updateCheck: vi.fn(async () => makeUpdate()) });
  Object.assign(navigator, { clipboard: clip });
  clip.writeText = vi.fn(async () => {});
  setLang('en');
  toasts.items = [];
  store.info = { version: '1.0.0', dataDir: '/d', platform: 'linux', nativeControls: false };
  store.settings = { language: 'en', run: { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } };
  store.state = 'stopped';
  store.players = [];
  store.now = Date.now();
  Object.assign(updates, {
    stage: 'idle', info: makeUpdate(), progress: null, failed: null, error: '', open: true, when: 'empty', scheduled: null, step: 0, done: null
  });
}
beforeEach(reset);
afterEach(() => {
  cleanup();
  updates.stop();
});

const dlg = () => screen.getByRole('dialog');

describe('UpdateDialog', () => {
  it('checks, then reports the newest version', async () => {
    updates.info = null;
    store.info = null;
    render(UpdateDialog);
    expect(dlg()).toHaveAccessibleName('Checking for updates…');
    expect(dlg()).toHaveAccessibleDescription('This takes a few seconds.');
    expect(screen.getByRole('progressbar', { name: 'Checking for updates…' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(updates.open).toBe(false);
    updates.open = true;
    updates.stage = 'uptodate';
    await Promise.resolve();
    store.info = { version: '1.0.0', dataDir: '/d', platform: 'linux', nativeControls: false };
    expect(await screen.findByText(/You have the newest version, 1\.0\.0/)).toBeInTheDocument();
    expect(screen.getByText('Up to date')).toHaveClass('ok');
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(updates.stage).toBe('idle');
  });

  it('shows an available update with its notes and downloads it', async () => {
    updates.stage = 'available';
    updates.info = makeUpdate({ prerelease: true }) as never;
    render(UpdateDialog);
    expect(dlg()).toHaveAccessibleName('Server 2.0.0 is here');
    expect(screen.getByText('You have 1.0.0.')).toBeInTheDocument();
    expect(screen.getByText('Test version')).toBeInTheDocument();
    const notes = screen.getByRole('region', { name: 'What’s new' });
    expect(notes).toHaveTextContent('New');
    expect(notes.querySelector('strong')).toHaveTextContent('Bold');
    expect(notes.querySelector('code')).toHaveTextContent('code');
    expect(notes).toHaveTextContent('See the page.');
    expect(screen.getByText(/Download size 32 MB/)).toBeInTheDocument();
    expect(screen.getByText(/Downloading doesn’t stop the server/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Full release notes' }));
    expect(mock.api.openUrl).toHaveBeenCalledWith(makeUpdate().page);
    await fireEvent.click(screen.getByRole('button', { name: 'Download update' }));
    expect(mock.api.updateDownload).toHaveBeenCalled();
  });

  it('sends Android and unverifiable releases to the browser', async () => {
    updates.stage = 'available';
    updates.info = makeUpdate({ mode: 'browser', asset: null, notes: '' }) as never;
    const r = render(UpdateDialog);
    expect(screen.queryByRole('region')).toBeNull();
    expect(screen.queryByText(/Download size/)).toBeNull();
    expect(screen.getByText('The download page opens in your browser.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open download page' }));
    expect(mock.api.openUrl).toHaveBeenCalledWith(makeUpdate().page);
    r.unmount();
    updates.open = true;
    store.info = { ...store.info!, platform: 'android' };
    const r2 = render(UpdateDialog);
    expect(screen.queryByText(/Android asks you to confirm/)).toBeNull();
    r2.unmount();
    updates.info = makeUpdate() as never;
    render(UpdateDialog);
    expect(screen.getByText(/Android asks you to confirm/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Download and install' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(updates.open).toBe(false);
  });

  it('shows the download progress', async () => {
    updates.stage = 'downloading';
    render(UpdateDialog);
    const bar = screen.getByRole('progressbar', { name: 'Download progress' });
    expect(bar).toHaveAttribute('aria-valuenow', '0');
    expect(screen.queryByText(/ of /)).toBeNull();
    updates.progress = { stage: 'downloading', done: 12 * 2 ** 20, total: 32 * 2 ** 20, bytesPerS: 2 ** 20, etaS: 20, error: null };
    await Promise.resolve();
    expect(await screen.findByText('12 MB of 32 MB · 1 MB/s · about 20 s left')).toBeInTheDocument();
    expect(bar).toHaveAttribute('aria-valuetext', '37%, about 20 seconds left');
    updates.progress = { ...updates.progress, done: 20 * 2 ** 20 };
    await Promise.resolve();
    await screen.findByText(/20 MB of 32 MB/);
    updates.progress = { ...updates.progress, stage: 'verifying', total: 0 };
    expect(await screen.findByText('Checking the download…')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel download' }));
    expect(mock.api.updateCancel).toHaveBeenCalled();
  });

  it('asks when to restart while players are online', async () => {
    updates.stage = 'ready';
    store.state = 'running';
    store.players = [player];
    render(UpdateDialog);
    expect(screen.getByText('The server must restart to use it.')).toBeInTheDocument();
    expect(screen.getByText('1 player is online.')).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'When should the server restart?' })).toBeInTheDocument();
    expect(screen.getByText(/Players see “Server restarts in 5 minutes for an update” in chat/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Schedule restart' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: /^Now/ }));
    expect(updates.when).toBe('now');
    expect(await screen.findByRole('button', { name: 'Restart now' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: /In 5 minutes/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Schedule restart' }));
    expect(updates.scheduled?.kind).toBe('warn');
    expect(mock.api.sendCommand).toHaveBeenCalled();
  });

  it('shows a scheduled restart and restarts right away without players', async () => {
    updates.stage = 'ready';
    store.state = 'running';
    updates.scheduled = { kind: 'warn', at: new Date(2026, 0, 1, 14, 5).getTime() };
    const r = render(UpdateDialog);
    expect(screen.getByText(/The server restarts at 14:05/)).toBeInTheDocument();
    updates.scheduled = { kind: 'empty', at: 0 };
    expect(await screen.findByText('The server restarts once the last player leaves.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Don’t restart' }));
    expect(updates.scheduled).toBeNull();
    r.unmount();
    updates.open = true;
    store.state = 'stopped';
    render(UpdateDialog);
    expect(screen.getByText('The app must restart to use it.')).toBeInTheDocument();
    expect(screen.queryByRole('group')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    expect(mock.api.updateInstall).toHaveBeenCalled();
  });

  it('restarts now from a scheduled restart and hides the popup with Later', async () => {
    updates.stage = 'ready';
    updates.scheduled = { kind: 'empty', at: 0 };
    render(UpdateDialog);
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    expect(mock.api.updateInstall).toHaveBeenCalled();
    updates.stage = 'ready';
    updates.scheduled = null;
    await fireEvent.click(await screen.findByRole('button', { name: 'Later' }));
    expect(updates.open).toBe(false);
  });

  it('walks through the install steps', async () => {
    updates.stage = 'installing';
    updates.info = makeUpdate({ mode: 'plugin' }) as never;
    store.state = 'running';
    render(UpdateDialog);
    expect(screen.getByText('Players can join again in about 30 seconds.')).toBeInTheDocument();
    expect(screen.getByText('World saved and backed up').tagName).toBe('STRONG');
    expect(screen.getAllByText('Installing 2.0.0')).toHaveLength(2);
    expect(screen.getByText('Starting the server again')).toBeInTheDocument();
    expect(screen.getByText('Don’t close the Server app until this finishes.')).toBeInTheDocument();
    store.state = 'stopped';
    updates.step = 3;
    updates.info = makeUpdate() as never;
    expect(await screen.findByText('The app restarts in a moment.')).toBeInTheDocument();
    expect(screen.getByText('Opening the installer for 2.0.0')).toBeInTheDocument();
    expect(screen.getByText('Starting the app again').tagName).toBe('STRONG');
    expect(screen.getByText(/The installer takes over/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Hide' }));
    expect(updates.open).toBe(false);
  });

  it('reports the update after the restart', async () => {
    updates.stage = 'done';
    updates.done = { version: '2.0.0', downtime: 24 };
    const r = render(UpdateDialog);
    expect(screen.getByText('Server 2.0.0 is running again. It was offline for 24 seconds.')).toBeInTheDocument();
    expect(screen.getByText('Updated')).toHaveClass('ok');
    await fireEvent.click(screen.getByRole('button', { name: 'What’s new' }));
    expect(mock.api.openUrl).toHaveBeenCalled();
    r.unmount();
    updates.open = true;
    updates.done = { version: '2.0.0', downtime: null };
    render(UpdateDialog);
    expect(screen.getByText('The app now runs version 2.0.0.')).toBeInTheDocument();
    updates.done = null;
    await Promise.resolve();
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(updates.stage).toBe('idle');
  });

  it('explains each kind of failure and copies the log', async () => {
    render(Toasts);
    updates.stage = 'error';
    updates.failed = 'check';
    updates.error = 'No connection.';
    const r = render(UpdateDialog);
    expect(dlg()).toHaveAccessibleName('Couldn’t check for updates');
    expect(screen.getByText('Offline')).toHaveClass('bad');
    expect(screen.getByRole('alert')).toHaveTextContent('No connection.');
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    expect(clip.writeText).toHaveBeenCalledWith(expect.stringContaining('No connection.'));
    expect(await screen.findByText('Copied to the clipboard')).toBeInTheDocument();
    clip.writeText = vi.fn(async () => { throw new Error('no'); });
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    expect(await screen.findByText(/Couldn’t copy/)).toBeInTheDocument();
    r.unmount();

    updates.open = true;
    updates.failed = 'download';
    updates.info = null;
    updates.error = '';
    const d = render(UpdateDialog);
    expect(dlg()).toHaveAccessibleName('The download didn’t finish');
    expect(screen.getByText('The server keeps running. You can try again.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    d.unmount();

    updates.open = true;
    updates.failed = 'install';
    updates.error = 'The installer could not be started.';
    store.state = 'running';
    const i = render(UpdateDialog);
    expect(dlg()).toHaveAccessibleName('The update was undone');
    expect(screen.getByRole('alert')).toHaveTextContent('The install didn’t finish. The server went back to 1.0.0 and is running.');
    i.unmount();
    updates.open = true;
    store.state = 'stopped';
    render(UpdateDialog);
    expect(screen.getByRole('alert')).toHaveTextContent('You still have 1.0.0.');
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mock.api.updateCheck).toHaveBeenCalled();
  });
});

describe('UpdateButton', () => {
  it('shows every state with a label', async () => {
    const r = render(UpdateButton);
    const b = () => screen.getByTestId('update-button');
    expect(b()).toHaveAccessibleName('Check for updates');
    expect(b()).toHaveAttribute('aria-haspopup', 'dialog');
    const cases: [string, Partial<typeof updates>, string, string][] = [
      ['checking', {}, 'Checking…', 'Checking…'],
      ['available', {}, 'Update available', 'Update available: 2.0.0'],
      ['downloading', { progress: { stage: 'downloading', done: 58, total: 100, bytesPerS: 0, etaS: 0, error: null } }, 'Downloading 58%', 'Downloading update, 58%'],
      ['ready', {}, 'Restart to update', 'Update ready, restart needed'],
      ['installing', {}, 'Installing…', 'Installing…'],
      ['error', { failed: 'check' }, 'Check for updates', 'Couldn’t check for updates'],
      ['error', { failed: 'install' }, 'Update failed', 'Last update failed'],
      ['done', {}, 'Check for updates', 'Check for updates']
    ];
    for (const [stage, extra, text, aria] of cases) {
      Object.assign(updates, { stage, ...extra });
      await Promise.resolve();
      await screen.findByText(text);
      expect(b()).toHaveAccessibleName(aria);
    }
    updates.stage = 'downloading';
    await Promise.resolve();
    expect(await screen.findByText(/Downloading/)).toBeInTheDocument();
    expect(b().querySelector('.ring')).not.toBeNull();
    updates.progress = { stage: 'downloading', done: 70, total: 100, bytesPerS: 0, etaS: 0, error: null };
    expect(await screen.findByText('Downloading 70%')).toBeInTheDocument();
    r.unmount();
  });

  it('has a rail icon and a More-list row, and opens the popup', async () => {
    updates.stage = 'available';
    updates.open = false;
    const r = render(UpdateButton, { variant: 'icon' });
    const b = screen.getByTestId('update-button');
    expect(b).toHaveAttribute('title', 'Update available: 2.0.0');
    expect(b.querySelector('.ib .pip.primary')).not.toBeNull();
    expect(b.querySelector('.lbl')).toBeNull();
    await fireEvent.click(b);
    expect(updates.open).toBe(true);
    updates.stage = 'idle';
    await waitFor(() => expect(b.querySelector('.pip')).toBeNull());
    r.unmount();
    render(UpdateButton, { variant: 'item' });
    expect(screen.getByTestId('update-button')).not.toHaveAttribute('title');
    await fireEvent.click(screen.getByTestId('update-button'));
    expect(mock.api.updateCheck).toHaveBeenCalled();
  });
});

describe('UpdateSettings', () => {
  it('shows the check result, last check and the options', async () => {
    render(UpdateSettings);
    expect(screen.getByText('Server 1.0.0')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Not checked yet');
    expect(screen.getByText('Never checked')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Check now' }));
    expect(mock.api.updateCheck).toHaveBeenCalled();
    expect(await screen.findByText('2.0.0 is available')).toBeInTheDocument();
    expect(screen.getByText('Last checked: just now')).toBeInTheDocument();
    updates.open = false;
    await fireEvent.click(screen.getByRole('button', { name: 'Show update' }));
    expect(updates.open).toBe(true);

    updates.stage = 'checking';
    expect(await screen.findByRole('button', { name: 'Check now' })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent('Checking…');
    updates.stage = 'uptodate';
    expect(await screen.findByText('Up to date')).toBeInTheDocument();
    updates.stage = 'error';
    updates.failed = 'check';
    expect(await screen.findByText('Couldn’t check — you may be offline')).toBeInTheDocument();
    expect(screen.getByText('Last successful check: just now')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    store.settings.updates = { ...DEFAULT_PREFS, lastChecked: 5 };
    expect(await screen.findByText('Never checked')).toBeInTheDocument();

    await fireEvent.click(screen.getByRole('switch', { name: 'Check for updates automatically' }));
    expect(updates.prefs.autoCheck).toBe(false);
    expect(screen.queryByRole('switch', { name: /Install/ })).toBeNull();
    await fireEvent.change(screen.getByRole('combobox', { name: 'Versions' }), { target: { value: 'beta' } });
    expect(updates.prefs.channel).toBe('beta');
    expect(mock.api.setSettings).toHaveBeenCalled();
  });

  it('copes with missing details', () => {
    store.info = null;
    updates.info = null;
    updates.stage = 'available';
    render(UpdateSettings);
    expect(screen.getByText('Server -')).toBeInTheDocument();
    expect(screen.getByText(/is available/)).toBeInTheDocument();
  });
});

describe('title bar chip', () => {
  it('is a button that reopens the popup and shows download progress', async () => {
    const onclick = vi.fn();
    const r = render(TitleBar, { os: 'linux', ctx: 'Dashboard', chip: { label: 'Updating 58%', tone: 'primary', pct: 58, onclick } });
    const chip = screen.getByRole('button', { name: 'Updating 58%: open the update window' });
    expect(chip).toHaveAttribute('aria-haspopup', 'dialog');
    expect(chip.querySelector('.cb')).toHaveStyle({ width: '58%' });
    await fireEvent.click(chip);
    expect(onclick).toHaveBeenCalled();
    await r.rerender({ os: 'linux', ctx: 'Dashboard', chip: { label: 'Updating 70%', tone: 'primary', pct: 70, onclick } });
    expect(document.querySelector('.chip .cb')).toHaveStyle({ width: '70%' });
    r.unmount();
    render(TitleBar, { os: 'linux', ctx: 'Dashboard', chip: { label: 'Restart to update', tone: 'ok', pct: null, onclick } });
    expect(document.querySelector('.chip .cb')).toBeNull();
  });
});
