import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('../api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from '../i18n.svelte';
import { W1, makeApi, makePoll } from '../mock-api';
import { store } from '../store.svelte';
import { setTheme, theme } from '../theme.svelte';
import { toasts } from '../ui/toast.svelte';
import Toasts from '../ui/Toasts.svelte';
import { viewport } from '../viewport.svelte';
import Access from './Access.svelte';
import Console from './Console.svelte';
import Dashboard from './Dashboard.svelte';
import More from './More.svelte';
import Players from './Players.svelte';
import Settings from './Settings.svelte';
import Worlds from './Worlds.svelte';

const W2 = 'b'.repeat(32);
const now = Date.now();
const bob = { name: 'bob', ip: '10.0.0.2', id: 'dev0123456789', joinedAt: now - 6 * 60000 };
const amy = { name: 'amy', ip: '10.0.0.3', id: 'dev9876543210', joinedAt: now - 2 * 3600000 };

function reset(over: Record<string, unknown> = {}) {
  mock.api = makeApi(over);
  Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => {}) } });
  setLang('en');
  setTheme('system');
  viewport.phone = false;
  viewport.rail = false;
  toasts.items = [];
  localStorage.clear();
  store.worlds = [{ id: W1, name: 'Sunny' }, { id: W2, name: 'Cave' }];
  store.error = '';
  store.state = 'stopped';
  store.detail = null;
  store.runningWorldId = null;
  store.startedAt = null;
  store.players = [];
  store.log = [];
  store.history = [];
  store.known = [];
  store.offline = false;
  store.now = now;
  store.info = { version: '1.0.0', dataDir: '/data', platform: 'linux', nativeControls: false };
  store.settings = { language: 'en', run: { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } };
  store.lists = { admin: ['amy'], mod: [], white: ['zed'], black: [] };
  store.welcome = 'Hello';
}
beforeEach(() => reset());
afterEach(() => cleanup());

describe('Dashboard', () => {
  it('shows a stopped server with the start action', async () => {
    render(Dashboard);
    expect(screen.getByTestId('state')).toHaveTextContent('Stopped');
    expect(screen.getByText('Start the server to let players join')).toBeInTheDocument();
    expect(screen.getByTestId('no-players')).toBeInTheDocument();
    expect(screen.getByText('Nothing logged yet.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Start server' }));
    await waitFor(() => expect(mock.api.startServer).toHaveBeenCalled());
  });

  it('shows a running server with address, players, roles and log', async () => {
    store.state = 'running';
    store.runningWorldId = W1;
    store.startedAt = now - 134 * 60000;
    store.settings.run.maxPlayers = 16;
    store.players = [bob, amy];
    store.log = [{ id: 1, at: now, level: 'info', text: 'hello' }];
    store.lists.mod = ['bob'];
    render(Dashboard);
    expect(screen.getByText('2h 14m')).toBeInTheDocument();
    expect(screen.getByText('/ 16')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('addr')).toHaveTextContent('192.168.1.5'));
    expect(screen.getAllByTestId('player')).toHaveLength(2);
    expect(screen.getByText('Admin')).toBeInTheDocument();
    expect(screen.getByText('Mod')).toBeInTheDocument();
    expect(screen.getByRole('log', { name: 'Latest console lines' })).toHaveTextContent('hello');
    await fireEvent.click(screen.getByRole('button', { name: 'Restart' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    await fireEvent.click(screen.getByRole('button', { name: /All players/ }));
    expect(store.screen).toBe('players');
  });

  it('copes with a missing network and no selected world', async () => {
    mock.api.networkInfo = vi.fn(async () => { throw new Error('x'); });
    store.worlds = [];
    store.state = 'running';
    render(Dashboard);
    await waitFor(() => expect(screen.getByTestId('no-net')).toBeInTheDocument());
    expect(screen.getAllByText('No world selected').length).toBeGreaterThan(0);
  });

  it('routes the failure banner actions', async () => {
    store.state = 'failed';
    store.detail = 'exit code 3';
    render(Dashboard);
    await fireEvent.click(screen.getByRole('button', { name: 'Change port' }));
    expect(store.screen).toBe('settings');
    await fireEvent.click(screen.getByRole('button', { name: 'Open log' }));
    expect(store.screen).toBe('console');
  });

  it('uses the phone layout with the world card and QR', () => {
    viewport.phone = true;
    store.state = 'running';
    render(Dashboard);
    expect(screen.getByRole('heading', { name: 'Server' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stop server' })).toBeInTheDocument();
  });
});

describe('Worlds', () => {
  it('lists worlds with status and filters them', async () => {
    store.state = 'running';
    store.runningWorldId = W1;
    store.players = [bob];
    store.settings.run.worldId = W2;
    render(Worlds);
    expect(screen.getAllByTestId('world')).toHaveLength(2);
    expect(screen.getByText('Loaded now')).toBeInTheDocument();
    expect(screen.getByText('Now · 1 player')).toBeInTheDocument();
    expect(screen.getByText('Selected')).toBeInTheDocument();
    expect(screen.getByText('Starts with the server')).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Search worlds'), { target: { value: 'zzz' } });
    expect(screen.getByTestId('no-match')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(screen.getAllByTestId('world')).toHaveLength(2);
  });

  it('loads another world', async () => {
    render(Worlds);
    // the selected world is the loaded one: a disabled green "Loaded"; the other can be loaded
    expect(screen.getByRole('button', { name: 'Loaded' })).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Load' }));
    await waitFor(() => expect(store.settings.run.worldId).toBe(W2));
  });

  it('shows the empty state and opens the create dialog from it', async () => {
    store.worlds = [];
    render(Worlds);
    expect(screen.getByRole('heading', { name: 'No worlds yet' })).toBeInTheDocument();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create world' })[1]);
    expect(screen.getByRole('dialog', { name: 'Create a world' })).toBeInTheDocument();
  });

  it('creates a world with the chosen options', async () => {
    mock.api.listWorlds = vi.fn(async () => [{ id: 'id1', name: 'MINE' }]);
    render(Worlds);
    render(Toasts);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create world' })[0]);
    const dlg = screen.getByRole('dialog');
    const submit = within(dlg).getByRole('button', { name: 'Create world' });
    expect(submit).toBeDisabled();
    await fireEvent.input(within(dlg).getByLabelText('World name'), { target: { value: 'mine' } });
    await fireEvent.click(within(dlg).getByRole('radio', { name: '4×' }));
    await fireEvent.click(within(dlg).getByRole('switch', { name: 'Expert mode' }));
    await fireEvent.click(within(dlg).getByLabelText('Use this world right away'));
    await fireEvent.click(submit);
    await waitFor(() => expect(mock.api.createWorld).toHaveBeenCalledWith({ name: 'mine', size: 'large', expert: true }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(await screen.findByText('World “mine” created')).toBeInTheDocument();
  });

  it('keeps the dialog open and shows the error when creating fails', async () => {
    mock.api.createWorld = vi.fn(async () => { throw 'create'; });
    store.state = 'running';
    render(Worlds);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create world' })[0]);
    const dlg = screen.getByRole('dialog');
    expect(within(dlg).getByLabelText('Load this world now (restarts the server)')).toBeChecked();
    await fireEvent.input(within(dlg).getByLabelText('World name'), { target: { value: 'x' } });
    await fireEvent.submit(dlg.closest('form')!);
    expect(await within(dlg).findByRole('alert')).toHaveTextContent('Could not create the world.');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('copies a world id and deletes after confirmation', async () => {
    render(Worlds);
    render(Toasts);
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Sunny' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Copy world ID' }));
    expect(await screen.findByText('World ID copied')).toBeInTheDocument();
    (navigator.clipboard.writeText as any).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Sunny' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Copy world ID' }));
    await screen.findByText(/Couldn’t copy/);
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Cave' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete world…' }));
    const dlg = screen.getByRole('dialog', { name: 'Delete “Cave”?' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    expect(mock.api.deleteWorld).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Cave' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete world…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Delete world' }));
    await waitFor(() => expect(mock.api.deleteWorld).toHaveBeenCalledWith(W2));
  });

  it('refuses to delete while the server runs', async () => {
    store.state = 'running';
    render(Worlds);
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Cave' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete world…' }));
    expect(screen.getByRole('button', { name: 'Delete world' })).toBeDisabled();
  });

  it('describes Android storage and hides desktop-only actions on phones', () => {
    store.info = { version: '1', dataDir: '/d', platform: 'android', nativeControls: false };
    viewport.phone = true;
    render(Worlds);
    expect(screen.queryByRole('button', { name: 'Import world' })).toBeNull();
    expect(screen.getByRole('heading', { name: 'Worlds' })).toBeInTheDocument();
  });
});

describe('Console screen', () => {
  it('toggles pause, saves the log and sends commands', async () => {
    store.state = 'running';
    store.log = [{ id: 100, at: now, level: 'info', text: 'hello' }];
    const createObjectURL = vi.fn(() => 'blob:x');
    const revoke = vi.fn();
    Object.assign(URL, { createObjectURL, revokeObjectURL: revoke });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    render(Console);
    expect(screen.getByText(/Live · 1 line/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Pause scrolling' }));
    expect(screen.getByRole('button', { name: 'Pause scrolling' })).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.click(screen.getByRole('button', { name: 'Save log' }));
    expect(click).toHaveBeenCalled();
    expect(revoke).toHaveBeenCalled();
    await fireEvent.input(screen.getByLabelText('Server command'), { target: { value: '/players' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(mock.api.sendCommand).toHaveBeenCalledWith('/players'));
    await fireEvent.keyDown(screen.getByLabelText('Server command'), { key: 'ArrowUp' });
    expect(screen.getByLabelText('Server command')).toHaveValue('/players');
    click.mockRestore();
  });

  it('shows the idle label when stopped and the phone layout', () => {
    render(Console);
    expect(screen.getByText(/Server stopped/)).toBeInTheDocument();
    cleanup();
    viewport.phone = true;
    render(Console);
    expect(screen.queryByText(/Server stopped ·/)).toBeNull();
  });
});

describe('Players', () => {
  beforeEach(() => {
    store.state = 'running';
    store.players = [bob, amy];
    store.lists.mod = ['bob'];
    store.known = [{ name: 'bob', lastSeen: now }, { name: 'old', lastSeen: now - 3 * 86400000 }, { name: 'zed', lastSeen: now - 3600000 }, { name: 'bad', lastSeen: now - 7200000 }, { name: 'amy', lastSeen: now }];
    store.lists.black = ['bad'];
  });

  it('shows the empty states', async () => {
    store.players = [];
    render(Players);
    expect(screen.getByText('Players appear here as soon as they join.')).toBeInTheDocument();
    cleanup();
    store.state = 'stopped';
    render(Players);
    await fireEvent.click(screen.getByRole('button', { name: 'Go to dashboard' }));
    expect(store.screen).toBe('dashboard');
  });

  it('lists online players with roles and the max', () => {
    store.settings.run.maxPlayers = 8;
    render(Players);
    expect(screen.getAllByTestId('player')).toHaveLength(2);
    expect(screen.getByText('2 of 8 online')).toBeInTheDocument();
    expect(screen.getByText('Admin')).toBeInTheDocument();
    expect(screen.getByText('Mod')).toBeInTheDocument();
    store.players = [{ ...bob, name: 'plain' }];
    cleanup();
    render(Players);
    expect(screen.getAllByText('Player').length).toBeGreaterThan(0);
  });

  it('promotes, demotes and copies ids', async () => {
    store.lists.mod = [];
    store.lists.admin = [];
    render(Players);
    render(Toasts);
    const open = async (name: string, item: string) => {
      await fireEvent.click(screen.getByRole('button', { name: `Actions for ${name}` }));
      await fireEvent.click(screen.getByRole('menuitem', { name: item }));
    };
    await open('bob', 'Make admin');
    await waitFor(() => expect(mock.api.setList).toHaveBeenCalledWith(W1, 'admin', ['bob']));
    expect(await screen.findByText('bob is now an admin')).toBeInTheDocument();
    await open('amy', 'Make mod');
    await waitFor(() => expect(mock.api.setList).toHaveBeenCalledWith(W1, 'mod', ['amy']));
    await open('bob', 'Remove admin');
    await waitFor(() => expect(store.lists.admin).toEqual([]));
    await open('amy', 'Remove mod');
    await open('bob', 'Copy device ID');
    expect(await screen.findByText('Device ID copied')).toBeInTheDocument();
    (navigator.clipboard.writeText as any).mockRejectedValueOnce(new Error('x'));
    await open('bob', 'Copy device ID');
    await screen.findByText(/Couldn’t copy/);
  });

  it('offers only fitting role actions', async () => {
    store.lists.admin = ['amy'];
    render(Players);
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for amy' }));
    expect(screen.queryByRole('menuitem', { name: 'Make mod' })).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Remove admin' })).toBeInTheDocument();
  });

  it('confirms kicks and bans', async () => {
    render(Players);
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Kick…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Kick player' }));
    await waitFor(() => expect(mock.api.sendCommand).toHaveBeenCalledWith('/kick bob'));
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for amy' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Ban…' }));
    await fireEvent.click(screen.getByLabelText(/Also block this IP address/));
    await fireEvent.click(screen.getByRole('button', { name: 'Ban player' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenCalledWith(W1, 'black', ['bad', 'amy']));
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Ban…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Ban player' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenLastCalledWith(W1, 'black', ['bad', 'amy', 'bob', '10.0.0.2']));
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Kick…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Ban…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  });

  it('browses and searches known players', async () => {
    render(Players);
    await fireEvent.click(screen.getByRole('tab', { name: /Known players/ }));
    expect(screen.getAllByTestId('known')).toHaveLength(5);
    expect(screen.getByText('Banned')).toBeInTheDocument();
    expect(screen.getByText('Whitelisted')).toBeInTheDocument();
    expect(screen.getAllByText('Online now')).toHaveLength(2);
    await fireEvent.input(screen.getByLabelText('Search known players'), { target: { value: 'zz' } });
    expect(screen.getByTestId('no-known')).toHaveTextContent('No players match');
    await fireEvent.input(screen.getByLabelText('Search known players'), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('tab', { name: /Online/ }));
    expect(screen.getAllByTestId('player')).toHaveLength(2);
  });

  it('expands long known lists and shows the empty message', async () => {
    store.known = Array.from({ length: 14 }, (_, i) => ({ name: `p${i}`, lastSeen: now }));
    render(Players);
    await fireEvent.click(screen.getByRole('tab', { name: /Known players/ }));
    expect(screen.getAllByTestId('known')).toHaveLength(12);
    await fireEvent.click(screen.getByRole('button', { name: 'Show all 14 known players' }));
    expect(screen.getAllByTestId('known')).toHaveLength(14);
    await fireEvent.click(screen.getByRole('button', { name: 'Show less' }));
    cleanup();
    store.known = [];
    render(Players);
    await fireEvent.click(screen.getByRole('tab', { name: /Known players/ }));
    expect(screen.getByTestId('no-known')).toHaveTextContent('Nobody has joined yet.');
  });
});

describe('Access', () => {
  it('shows the four lists and edits them', async () => {
    store.state = 'running';
    render(Access);
    render(Toasts);
    expect(screen.getByText('The server is running: changes are applied through console commands.')).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Add admin'), { target: { value: 'Bob' } });
    await fireEvent.click(screen.getAllByRole('button', { name: 'Add' })[0]);
    await waitFor(() => expect(mock.api.setList).toHaveBeenCalledWith(W1, 'admin', ['amy', 'Bob']));
    expect(await screen.findByText('Bob added to Admins')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenLastCalledWith(W1, 'admin', ['amy']));
    await fireEvent.input(screen.getByLabelText('Ban a name or IP address'), { target: { value: '1.2.3.4' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Ban' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenLastCalledWith(W1, 'black', ['1.2.3.4']));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove zed from whitelist' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenLastCalledWith(W1, 'white', []));
    await fireEvent.input(screen.getByLabelText('Add admin'), { target: { value: 'AMY' } });
    await fireEvent.click(screen.getAllByRole('button', { name: 'Add' })[0]);
  });

  it('asks for a world first', async () => {
    store.settings.run.worldId = '';
    render(Access);
    await fireEvent.click(screen.getByRole('button', { name: 'Worlds' }));
    expect(store.screen).toBe('worlds');
  });

  it('hides import on phones', () => {
    viewport.phone = true;
    render(Access);
    expect(screen.queryByRole('button', { name: 'Import lists' })).toBeNull();
  });
});

describe('Settings', () => {
  it('edits the selected world\'s welcome message', async () => {
    render(Settings);
    render(Toasts);
    expect(screen.getByText(/Players see it when they join Sunny/)).toBeInTheDocument();
    const box = screen.getByRole('textbox', { name: 'Welcome message' }) as HTMLTextAreaElement;
    expect(box.value).toBe('Hello');
    const save = screen.getByRole('button', { name: 'Save message' });
    expect(save).toBeDisabled();
    await fireEvent.input(box, { target: { value: 'Hi <b>all</b>\n' } });
    expect(screen.getByText('14/4000')).toBeInTheDocument();
    await fireEvent.click(save);
    await waitFor(() => expect(mock.api.setWelcome).toHaveBeenCalledWith(W1, 'Hi <b>all</b>\n'));
    expect(await screen.findByText('Welcome message saved')).toBeInTheDocument();
    expect(store.welcome).toBe('Hi <b>all</b>');
    expect(save).toBeDisabled();
    // A failure keeps the text and shows the reason.
    mock.api.setWelcome = vi.fn(async () => { throw 'tooLong'; });
    await fireEvent.input(box, { target: { value: 'x' } });
    await fireEvent.click(save);
    await waitFor(() => expect(store.error).toBe('The message is too long (4000 characters at most).'));
    expect(box.value).toBe('x');
  });

  it('has no welcome message without a world', () => {
    store.worlds = [];
    render(Settings);
    expect(screen.queryByRole('textbox', { name: 'Welcome message' })).toBeNull();
  });

  it('validates and saves connection settings', async () => {
    render(Settings);
    render(Toasts);
    expect(screen.queryByRole('region', { name: 'Unsaved changes' })).toBeNull();
    const port = screen.getByLabelText('Port');
    await fireEvent.input(port, { target: { value: '80' } });
    expect(screen.getByText('Use a number from 1024 to 65535.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    await fireEvent.input(port, { target: { value: '16000' } });
    await fireEvent.input(screen.getByLabelText('Max players', { selector: 'input[type=text]' }), { target: { value: '99' } });
    expect(screen.getByText('Use a number from 0 to 64.')).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Max players', { selector: 'input[type=text]' }), { target: { value: 'x' } });
    await fireEvent.input(screen.getByLabelText('Max players', { selector: 'input[type=text]' }), { target: { value: '12' } });
    await fireEvent.input(screen.getByRole('slider'), { target: { value: '20' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'x'.repeat(16) } });
    expect(screen.getByText('Use 15 characters or fewer.')).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Show password' }));
    expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'text');
    await fireEvent.input(screen.getByLabelText('Save delay'), { target: { value: '0' } });
    expect(screen.getByText('Use a number from 1 to 3600.')).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Save delay'), { target: { value: '30' } });
    expect(screen.getByText('4 unsaved changes')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(store.settings.run).toMatchObject({ port: 16000, maxPlayers: 20, password: 'secret', saveDelay: 30 }));
    expect(await screen.findByText('Settings saved')).toBeInTheDocument();
  });

  it('turns the same-name rule off and on again (on when the setting is missing)', async () => {
    render(Settings);
    const sw = screen.getByRole('switch', { name: 'Same name from any device' }) as HTMLInputElement;
    expect(sw.checked).toBe(true);
    await fireEvent.click(sw);
    expect(screen.getByText('1 unsaved change')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(sw.checked).toBe(true);
    await fireEvent.click(sw);
    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(store.settings.run.sharedNames).toBe(false));
    expect(sw.checked).toBe(false);
    expect(screen.queryByRole('region', { name: 'Unsaved changes' })).toBeNull();
  });

  it('discards edits and restarts a running server on save', async () => {
    store.state = 'running';
    render(Settings);
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '16000' } });
    expect(screen.getByText('1 unsaved change · the server restarts when you save')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(screen.getByLabelText('Port')).toHaveValue('15151');
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '16001' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save & restart' }));
    await waitFor(() => expect(mock.api.startServer).toHaveBeenCalledWith(expect.objectContaining({ port: 16001 })));
  });

  it('does not toast when saving fails', async () => {
    mock.api.setSettings = vi.fn(async () => { throw 'running'; });
    render(Settings);
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '16000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(store.error).toBe('Stop the server first.'));
    expect(toasts.items).toHaveLength(0);
  });

  it('switches language and theme and copies the data folder', async () => {
    render(Settings);
    render(Toasts);
    await fireEvent.change(screen.getByLabelText('Language'), { target: { value: 'tr' } });
    await waitFor(() => expect(screen.getByRole('heading', { name: 'Ayarlar' })).toBeInTheDocument());
    expect(mock.api.setSettings).toHaveBeenCalledWith(expect.objectContaining({ language: 'tr' }));
    await fireEvent.click(screen.getByRole('radio', { name: 'Koyu' }));
    expect(theme.choice).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    await fireEvent.click(screen.getByRole('button', { name: 'Yolu kopyala' }));
    expect(await screen.findByText('Yol kopyalandı')).toBeInTheDocument();
    (navigator.clipboard.writeText as any).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'Yolu kopyala' }));
    await screen.findByText(/Kopyalanamadı/);
  });

  it('hides desktop-only settings on Android and handles a missing info', () => {
    store.info = { version: '1', dataDir: '/d', platform: 'android', nativeControls: false };
    render(Settings);
    expect(screen.queryAllByText('Start on boot')).toHaveLength(0);
    cleanup();
    store.info = null;
    render(Settings);
    expect(screen.getAllByText('Start on boot').length).toBeGreaterThan(0);
    expect(screen.getAllByText('-').length).toBeGreaterThan(0);
  });

  it('renders the disabled desktop toggles', () => {
    render(Settings);
    for (const n of ['Start on boot', 'Keep running when the window is closed', 'Browser admin panel']) expect(screen.getByRole('switch', { name: n })).toBeDisabled();
  });
});

describe('More', () => {
  it('links to the screens outside the bottom bar', async () => {
    render(More);
    await fireEvent.click(screen.getByRole('button', { name: /Access lists/ }));
    expect(store.screen).toBe('access');
    await fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(store.screen).toBe('settings');
  });
});

describe('dialogs and callbacks', () => {
  it('closes dialogs with Escape', async () => {
    store.state = 'running';
    store.players = [bob];
    render(Players);
    for (const item of ['Kick…', 'Ban…']) {
      await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: item }));
      await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
      expect(screen.queryByRole('dialog')).toBeNull();
    }
    cleanup();
    render(Worlds);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create world' })[0]);
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Cave' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete world…' }));
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('does not announce a promotion that failed', async () => {
    mock.api.setList = vi.fn(async () => { throw 'stopped'; });
    store.state = 'running';
    store.players = [bob];
    render(Players);
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Make admin' }));
    await waitFor(() => expect(store.error).toBe('The server is not running.'));
    expect(toasts.items).toHaveLength(0);
  });

  it('undoes a promotion from the toast', async () => {
    store.state = 'running';
    store.players = [bob];
    store.lists.admin = [];
    render(Players);
    render(Toasts);
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for bob' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Make mod' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenLastCalledWith(W1, 'mod', []));
  });

  it('stops a running server and retries a lost connection from the dashboard', async () => {
    store.state = 'running';
    store.offline = true;
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', worldId: W1 }));
    render(Dashboard);
    await fireEvent.click(screen.getByRole('button', { name: 'Retry now' }));
    await waitFor(() => expect(mock.api.poll).toHaveBeenCalled());
    await fireEvent.click(screen.getByRole('button', { name: 'Stop server' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
  });

  it('shows the loading banner with the world name while starting', () => {
    store.state = 'starting';
    store.runningWorldId = W1;
    render(Dashboard);
    expect(screen.getAllByText('Loading world “Sunny”…').length).toBeGreaterThan(0);
  });

  it('unbans from the blacklist', async () => {
    store.lists.black = ['bad'];
    render(Access);
    await fireEvent.click(screen.getByRole('button', { name: 'Unban bad' }));
    await waitFor(() => expect(mock.api.setList).toHaveBeenCalledWith(W1, 'black', []));
  });

  it('copes with copying the data folder before app info exists', async () => {
    store.info = null;
    render(Settings);
    render(Toasts);
    await fireEvent.click(screen.getByRole('button', { name: 'Copy path' }));
    expect(await screen.findByText('Path copied')).toBeInTheDocument();
  });
});
