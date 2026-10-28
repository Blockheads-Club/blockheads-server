import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('../api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { fileSize } from '../format';
import { setLang, t } from '../i18n.svelte';
import { makeApi, makeMod, makeOverview, makePoll } from '../mock-api';
import Mods from '../screens/Mods.svelte';
import { store } from '../store.svelte';
import type { BrowseMod, ModsOverview, Plan, PlanItem } from '../types';
import { toasts } from '../ui/toast.svelte';
import { viewport } from '../viewport.svelte';
import BudgetMeter from './BudgetMeter.svelte';
import ModIcon, { initials, toneOf } from './ModIcon.svelte';
import ModsCard from './ModsCard.svelte';
import { isModError, loc, modErr, mods, sidesOf } from './mods.svelte';
import { permInfo } from './perms';

function overview(over: Partial<ModsOverview> = {}) {
  return makeOverview(over);
}

function item(over: Partial<PlanItem> = {}): PlanItem {
  return {
    id: 'auction-house',
    name: 'Auction House',
    version: '1.3.0',
    size: 186000,
    sides: ['server'],
    clientRequired: false,
    permissions: ['players:inventory', 'players:chat'],
    native: false,
    neededBy: null,
    replaces: null,
    reviewed: true,
    ...over
  };
}

function plan(over: Partial<Plan> = {}): Plan {
  return { items: [item()], satisfied: [], missing: [], totalBytes: 186000, ...over };
}

function browseMod(over: Partial<BrowseMod> = {}): BrowseMod {
  return {
    id: 'economy-core',
    name: 'Economy Core',
    description: 'Coins and wallets.',
    owners: ['mira_k'],
    repo: 'https://x',
    tags: ['economy'],
    version: '2.3.1',
    sides: ['server'],
    clientRequired: false,
    permissions: ['players:chat'],
    native: false,
    depends: {},
    size: 226000,
    published: '2026-10-19',
    firstPublished: '2026-09-01',
    installed: null,
    ...over
  };
}

/** The demo set of the design: one mod per status. */
const demo = () => [
  makeMod({ id: 'better-chat', name: { en: 'Better Chat' }, authors: ['mira_k'] }),
  makeMod({ id: 'land-claims', update: '1.5.0', homepage: null, authors: [] }),
  makeMod({ id: 'minimap', name: { en: 'Minimap' }, sides: { server: true, client: true, clientRequired: true, clientRequiredDefault: true, clientRequiredLocked: false } }),
  makeMod({ id: 'weather-plus', name: { en: 'Weather Plus' }, status: 'budget', detail: '3 faults within 60 s', slowestMs: 7.9 }),
  makeMod({
    id: 'shop-signs',
    name: { en: 'Shop Signs' },
    status: 'dependency',
    detail: 'economy-core ^2',
    dependencies: [
      { id: 'economy-core', req: '^2', installed: null, ok: false, soft: false },
      { id: 'nice', req: '1', installed: null, ok: false, soft: true }
    ]
  }),
  makeMod({ id: 'discord-bridge', name: { en: 'Discord Bridge' }, status: 'permissions', permissions: ['players:chat', 'net-http:discord.com'], granted: ['players:chat'], missing: ['net-http:discord.com'] }),
  makeMod({ id: 'fast-tools', name: { en: 'Fast Tools' }, status: 'failed', detail: 'missing symbol', native: true, permissions: ['native'], granted: ['native'], reviewed: false }),
  makeMod({ id: 'hats', name: { en: 'Hats & Capes' }, status: 'off', enabled: false, sides: { server: false, client: true, clientRequired: true, clientRequiredDefault: true, clientRequiredLocked: false } })
];

function reset(over: Record<string, unknown> = {}) {
  // Reloading keeps what the test set up.
  mock.api = makeApi({ modsOverview: vi.fn(async () => mods.overview ?? makeOverview()), ...over });
  Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => {}) } });
  setLang('en');
  viewport.phone = false;
  toasts.items = [];
  store.state = 'stopped';
  store.players = [];
  store.screen = 'mods';
  store.restartWhenEmpty = false;
  store.consoleQuery = '';
  store.info = { version: '1.0.0', dataDir: '/data', platform: 'linux', nativeControls: false };
  Object.assign(mods, {
    overview: null,
    loadError: null,
    error: '',
    view: 'list',
    detailId: null,
    query: '',
    filter: 'all',
    side: 'all',
    busy: '',
    browse: null,
    browseLoading: false,
    browseError: null,
    bquery: '',
    bside: 'any',
    hideNative: false,
    sort: 'updated',
    tag: '',
    flow: null
  });
}
// Many renders per test: slow under coverage on a busy machine.
vi.setConfig({ testTimeout: 30000 });
beforeEach(() => reset());
afterEach(() => cleanup());

describe('helpers', () => {
  it('names, sides, errors and permission tiers', () => {
    expect(loc({ en: 'A', tr: 'B' }, 'x')).toBe('A');
    setLang('tr');
    expect(loc({ en: 'A', tr: 'B' }, 'x')).toBe('B');
    expect(loc({ de: 'C' }, 'x')).toBe('C');
    expect(loc({}, 'x')).toBe('x');
    setLang('en');
    expect([sidesOf(true, false), sidesOf(false, true), sidesOf(true, true)]).toEqual(['server', 'client', 'both']);
    expect(isModError({ code: 'x' })).toBe(true);
    expect(isModError('x')).toBe(false);
    expect(isModError(null)).toBe(false);
    expect(modErr({ code: 'running' })).toBe('Stop the server first.');
    expect(modErr({ code: 'download', detail: 'timeout' })).toBe('The download failed: timeout');
    expect(modErr({ code: 'weird' })).toBe('Something went wrong: weird');
    expect(modErr({ code: 'weird', detail: 'd' })).toBe('Something went wrong: d');
    expect(modErr({ code: 'missing' })).toBe('The mod “” is no longer installed.');
    expect(modErr('running')).toBe(t('err.running'));
    expect(permInfo('net-http:discord.com')).toEqual({ key: 'net', tier: 1, icon: 'globe', host: 'discord.com' });
    expect(permInfo('players:kick').tier).toBe(2);
    expect(permInfo('bogus')).toEqual({ key: 'other', tier: 1, icon: 'info' });
    expect(initials('Hats & Capes')).toBe('HC');
    expect(initials('minimap')).toBe('MI');
    expect(initials('&&')).toBe('?');
    expect(toneOf('a')).toBe(toneOf('a'));
    expect(fileSize(186000)).toBe('182 kB');
    expect(fileSize(2.1 * 2 ** 20)).toBe('2.1 MB');
    expect(fileSize(0)).toBe('0 kB');
  });

  it('draws icons and meters', () => {
    const { container } = render(ModIcon, { id: 'x', name: 'Land Claims', size: 'lg' });
    expect(container.querySelector('.mico.lg')).toHaveTextContent('LC');
    cleanup();
    render(BudgetMeter, { usedMs: 7.9, budgetMs: 5, maxMs: 25, label: 'Slowest call' });
    const m = screen.getByRole('meter', { name: 'Slowest call' });
    expect(m).toHaveAttribute('aria-valuetext', '7.9 ms of 5.0 ms budget per call');
    expect(m.querySelector('.over')).not.toBeNull();
    cleanup();
    render(BudgetMeter, { usedMs: 99, budgetMs: 5, maxMs: 25, label: 'x' });
    expect(screen.getByRole('meter').querySelector('span')).toHaveStyle('width: 100%');
  });
});

describe('store', () => {
  it('shares one refresh and records load errors', async () => {
    const o = overview({ mods: demo() });
    mock.api.modsOverview = vi.fn(async () => o);
    await Promise.all([mods.refresh(), mods.refresh()]);
    expect(mock.api.modsOverview).toHaveBeenCalledTimes(1);
    expect(mods.attention.map((m) => m.id)).toEqual(['weather-plus', 'shop-signs', 'discord-bridge', 'fast-tools']);
    expect(mods.waiting.map((m) => m.id)).toEqual(['shop-signs', 'discord-bridge']);
    expect(mods.detail).toBeUndefined();
    mock.api.modsOverview = vi.fn(async () => { throw { code: 'io', detail: 'denied' }; });
    await mods.refresh();
    expect(mods.loadError).toEqual({ code: 'io', detail: 'denied' });
    mock.api.modsOverview = vi.fn(async () => { throw new Error('boom'); });
    await mods.refresh();
    expect(mods.loadError).toEqual({ code: 'other', detail: 'Error: boom' });
    expect(mods.mods).toHaveLength(8);
  });

  it('filters the installed list and the public list', () => {
    mods.overview = overview({ mods: demo(), clientMods: ['minimap'] });
    mods.filter = 'running';
    expect(mods.visible.map((m) => m.id)).toEqual(['better-chat', 'land-claims', 'minimap']);
    mods.filter = 'off';
    expect(mods.visible.map((m) => m.id)).toEqual(['hats']);
    mods.filter = 'updates';
    expect(mods.visible.map((m) => m.id)).toEqual(['land-claims']);
    mods.filter = 'client';
    expect(mods.visible.map((m) => m.id)).toEqual(['minimap']);
    mods.filter = 'all';
    mods.side = 'client';
    expect(mods.visible.map((m) => m.id)).toEqual(['hats']);
    mods.side = 'all';
    mods.query = 'TOBY';
    expect(mods.visible.length).toBe(6);

    mods.browse = {
      generated: '2026-10-21T00:00:00Z',
      offline: false,
      error: null,
      hidden: 0,
      mods: [
        browseMod({ id: 'b', name: 'Beta', published: '2026-10-01', firstPublished: '2026-01-01', tags: ['chat'] }),
        browseMod({ id: 'a', name: 'Alpha', published: '2026-09-01', firstPublished: '2026-05-01', native: true, clientRequired: true, description: null })
      ]
    };
    expect(mods.browseList.map((m) => m.id)).toEqual(['b', 'a']);
    expect(mods.tags).toEqual(['chat', 'economy']);
    mods.sort = 'name';
    expect(mods.browseList.map((m) => m.id)).toEqual(['a', 'b']);
    mods.sort = 'newest';
    expect(mods.browseList.map((m) => m.id)).toEqual(['a', 'b']);
    mods.bside = 'server';
    expect(mods.browseList.map((m) => m.id)).toEqual(['b']);
    mods.bside = 'client';
    expect(mods.browseList.map((m) => m.id)).toEqual(['a']);
    mods.hideNative = true;
    expect(mods.browseList).toEqual([]);
    mods.bside = 'any';
    mods.hideNative = false;
    mods.tag = 'chat';
    mods.bquery = 'bet';
    expect(mods.browseList.map((m) => m.id)).toEqual(['b']);
    mods.browse = null;
    expect(mods.browseList).toEqual([]);
    expect(mods.tags).toEqual([]);
  });

  it('runs actions and reports failures', async () => {
    const m = demo()[0];
    expect(await mods.toggle(m, false)).toBe(true);
    expect(mock.api.modsSetEnabled).toHaveBeenCalledWith('better-chat', false);
    expect(toasts.items[0].message).toBe('Better Chat is off');
    await mods.toggle(m, true);
    expect(toasts.items[1].message).toBe('Better Chat is on');
    await mods.grant(m, 'players:chat', true);
    expect(mock.api.modsSetGrant).toHaveBeenCalledWith('better-chat', 'players:chat', true);
    mods.view = 'detail';
    await mods.remove(m);
    expect(mock.api.modsRemove).toHaveBeenCalledWith('better-chat-1.4.2.bhmod');
    expect(mods.view).toBe('list');
    await mods.removeFile('junk.bhmod');
    expect(mock.api.modsRemove).toHaveBeenCalledWith('junk.bhmod');
    await mods.clearData(m);
    expect(mock.api.modsClearData).toHaveBeenCalledWith('better-chat');
    await mods.openFolder('better-chat');
    expect(mock.api.modsOpenFolder).toHaveBeenCalledWith('better-chat');
    mock.api.modsRemove = vi.fn(async () => { throw { code: 'running' }; });
    mods.view = 'detail';
    expect(await mods.remove(m)).toBe(false);
    expect(mods.view).toBe('detail');
    expect(mods.error).toBe('Stop the server first.');
  });

  it('navigates between the list, a mod and the public list', async () => {
    store.screen = 'dashboard';
    mods.open('minimap');
    expect([store.screen, mods.view, mods.detailId]).toEqual(['mods', 'detail', 'minimap']);
    mods.showList('attention');
    expect([mods.view, mods.filter]).toEqual(['list', 'attention']);
    mods.showList();
    expect(mods.filter).toBe('attention');
    mods.showBrowse('chat');
    expect([mods.view, mods.bquery]).toEqual(['browse', 'chat']);
    await waitFor(() => expect(mods.browse).not.toBeNull());
    mods.showBrowse();
    expect(mock.api.modsBrowse).toHaveBeenCalledTimes(1);
    mock.api.modsBrowse = vi.fn(async () => { throw 'nope'; });
    await mods.loadBrowse();
    expect(mods.browseError).toEqual({ code: 'other', detail: 'nope' });
    mock.api.modsBrowse = vi.fn(async () => { throw { code: 'offline' }; });
    await mods.loadBrowse();
    expect(mods.browseError).toEqual({ code: 'offline' });
  });

  it('install flow: plan, permissions, native confirm, progress and result', async () => {
    const p = plan({ items: [item(), item({ id: 'eco', name: 'Eco', neededBy: 'auction-house', native: true, permissions: ['native', 'players:chat'] })] });
    mock.api.modsPlan = vi.fn(async () => p);
    let progress: ((x: unknown) => void) | undefined;
    mock.api.onModsProgress = vi.fn(async (cb: (x: unknown) => void) => ((progress = cb), () => {}));
    mock.api.modsInstall = vi.fn(async () => {
      progress!({ step: 'download', done: 0, total: 2 });
      return { ids: ['auction-house', 'eco'], restart: true };
    });
    mods.browse = { generated: '', offline: false, error: null, hidden: 0, mods: [] };
    await mods.install('auction-house');
    expect(mods.flow!.stage).toBe('plan');
    expect(mods.flow!.grants).toEqual({ 'auction-house': ['players:inventory', 'players:chat'], eco: ['players:chat'] });
    mods.setGrant('auction-house', 'players:chat', false);
    mods.setGrant('eco', 'players:kick', true);
    mods.setGrant('new', 'x', false);
    expect(mods.flow!.grants['new']).toEqual([]);
    await mods.confirm(true);
    expect(mods.flow!.stage).toBe('native');
    await mods.confirm(true);
    expect(mods.flow!.stage).toBe('done');
    expect(mock.api.modsInstall).toHaveBeenCalledWith({
      items: [{ id: 'auction-house', version: '1.3.0' }, { id: 'eco', version: '1.3.0' }],
      grants: { 'auction-house': ['players:inventory'], eco: ['players:chat', 'players:kick', 'native'], new: [] },
      enable: true,
      local: false
    });
    expect(mods.flow!.progress).toEqual({ step: 'download', done: 0, total: 2 });
    expect(mock.api.modsBrowse).toHaveBeenCalled();
    mods.close();
    expect(mods.flow).toBeNull();
    mods.close();
  });

  it('install flow: one mod goes straight to the permissions; failures and retries', async () => {
    mock.api.modsPlan = vi.fn(async () => plan());
    mock.api.onModsProgress = vi.fn(async () => { throw new Error('no events'); });
    mock.api.modsInstall = vi.fn(async () => { throw { code: 'checksum', file: 'a.bhmod', expected: 'x', got: 'y' }; });
    await mods.install('auction-house', '1.3.0');
    expect(mods.flow!.stage).toBe('perms');
    await mods.confirm(false);
    expect(mods.flow!.stage).toBe('failed');
    expect(mods.flow!.error!.code).toBe('checksum');
    mock.api.modsPlan = vi.fn(async () => { throw 'gone'; });
    await mods.retry();
    expect(mock.api.modsPlan).toHaveBeenCalledWith('auction-house', '1.3.0');
    expect(mods.flow!.error).toEqual({ code: 'other', detail: 'gone' });

    // While installing the dialog stays.
    mods.flow!.stage = 'installing';
    mods.close();
    expect(mods.flow).not.toBeNull();

    // Files and links.
    mods.openFile();
    expect(mods.flow!.stage).toBe('file');
    const file = new File([new Uint8Array([1, 2, 3])], 'fast.bhmod');
    await mods.pickFile(file);
    expect(mock.api.modsInspectFile).toHaveBeenCalledWith('fast.bhmod', new Uint8Array([1, 2, 3]));
    expect(mods.flow!.stage).toBe('perms');
    mock.api.modsInspectFile = vi.fn(async () => { throw { code: 'invalid', detail: 'no mod.toml' }; });
    await mods.pickFile(file);
    expect(mods.flow!.stage).toBe('failed');
    await mods.retry();
    expect(mods.flow!.stage).toBe('file');
    mock.api.modsInspectUrl = vi.fn(async () => { throw { code: 'insecure' }; });
    await mods.pickUrl(' http://x/a.bhmod ');
    expect(mock.api.modsInspectUrl).toHaveBeenCalledWith('http://x/a.bhmod');
    mock.api.modsInspectUrl = vi.fn(async () => ({ source: 'a', sha256: 'f', plan: plan() }));
    await mods.retry();
    expect(mods.flow!.stage).toBe('perms');
    expect(mods.flow!.origin).toEqual({ kind: 'local', url: ' http://x/a.bhmod ' });
    mods.flow = null;
    expect(mods.nativeItems).toEqual([]);
  });
});

describe('Mods list', () => {
  it('shows the skeleton, then the empty state with its actions', async () => {
    render(Mods);
    expect(screen.getByRole('status')).toHaveTextContent('Loading mods…');
    expect(screen.getByRole('heading', { name: 'Mods' }).nextElementSibling).toHaveTextContent('Installed mods');
    mods.overview = overview();
    await waitFor(() => expect(screen.getByRole('heading', { name: 'No mods yet' })).toBeInTheDocument());
    expect(screen.getByText('Add features to your server')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Install from a file or link' }));
    expect(mods.flow!.stage).toBe('file');
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Browse the mod list' }));
    expect(mods.view).toBe('browse');
  });

  it('lists mods with their status, fixes and filters', async () => {
    mods.overview = overview({ mods: demo(), clientMods: ['minimap', 'hats'], size: 2 ** 20 });
    render(Mods);
    expect(screen.getByText('8 installed · 3 running · 4 need attention')).toBeInTheDocument();
    expect(screen.getAllByTestId('mod-row')).toHaveLength(8);
    expect(screen.getByText('Players need the mod loader to join this server')).toBeInTheDocument();
    expect(screen.getByText(/Minimap, Hats & Capes also runs/)).toBeInTheDocument();
    expect(screen.getByText('Stopped after 3 faults within 60 s. Turn it on again to retry.')).toBeInTheDocument();
    expect(screen.getByText('Needs economy-core ^2, which is not installed.')).toBeInTheDocument();
    expect(screen.getByText('Asks for Internet: discord.com. Not granted yet.')).toBeInTheDocument();
    expect(screen.getByText('Stopped: “missing symbol”')).toBeInTheDocument();
    expect(screen.getAllByRole('img', { name: 'Native code: full access, not sandboxed' })).toHaveLength(1);
    expect(screen.getByText('Update 1.5.0')).toBeInTheDocument();
    expect(screen.getByText(/1\.0 MB/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'See performance' }));
    expect(mods.detailId).toBe('weather-plus');
    mods.view = 'list';
    await fireEvent.click(await screen.findByRole('button', { name: 'Install economy-core' }));
    expect(mock.api.modsPlan).toHaveBeenCalledWith('economy-core', undefined);
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Review permissions' }));
    expect(mods.detailId).toBe('discord-bridge');
    mods.view = 'list';
    await fireEvent.click(await screen.findByRole('button', { name: 'See error' }));
    expect(mods.detailId).toBe('fast-tools');
    mods.view = 'list';
    await fireEvent.click(await screen.findByRole('button', { name: 'Minimap details' }));
    expect(mods.detailId).toBe('minimap');
    mods.view = 'list';
    await fireEvent.click(await screen.findByRole('switch', { name: 'Better Chat on or off' }));
    await waitFor(() => expect(mock.api.modsSetEnabled).toHaveBeenCalledWith('better-chat', false));

    await fireEvent.click(screen.getByRole('button', { name: 'Show client mods (2)' }));
    expect(screen.getAllByTestId('mod-row')).toHaveLength(2);
    await fireEvent.click(screen.getByRole('button', { name: /^Client mods/ }));
    expect(mods.filter).toBe('all');
    await fireEvent.click(screen.getByRole('button', { name: /^Needs attention/ }));
    expect(screen.getAllByTestId('mod-row')).toHaveLength(4);
    for (const f of ['Running', 'Off', 'Updates', 'All']) await fireEvent.click(screen.getByRole('button', { name: new RegExp(`^${f}`) }));
    await fireEvent.change(screen.getByLabelText('Sides'), { target: { value: 'both' } });
    expect(screen.getAllByTestId('mod-row')).toHaveLength(1);
    await fireEvent.input(screen.getByLabelText('Search installed mods'), { target: { value: 'zzz' } });
    expect(screen.getByText('No mods match “zzz”')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Search the mod list' }));
    expect([mods.view, mods.bquery]).toEqual(['browse', 'zzz']);
    mods.view = 'list';
    await fireEvent.click(await screen.findByRole('button', { name: 'Clear filters' }));
    expect(screen.getAllByTestId('mod-row')).toHaveLength(8);
    mods.filter = 'updates';
    mods.side = 'client';
    await waitFor(() => expect(screen.getByText('No mods here')).toBeInTheDocument());

    await fireEvent.click(screen.getByRole('button', { name: 'Open folder' }));
    expect(mock.api.modsOpenFolder).toHaveBeenCalledWith(undefined);
    await fireEvent.click(screen.getByRole('button', { name: 'Install from file or link' }));
    expect(mods.flow).toMatchObject({ stage: 'file' });
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Browse mod list' }));
    expect(mods.view).toBe('browse');
  });

  it('dependency rows for an installed but unusable dependency, restart rows', () => {
    mods.overview = overview({
      mods: [
        makeMod({ id: 'a', status: 'dependency', dependencies: [{ id: 'b', req: '^2', installed: '1.0.0', ok: false, soft: false }] }),
        makeMod({ id: 'c', status: 'restart' }),
        makeMod({ id: 'd', status: 'dependency', dependencies: [] }),
        makeMod({ id: 'e', status: 'budget', detail: null }),
        makeMod({ id: 'f', status: 'failed', detail: null })
      ]
    });
    render(Mods);
    expect(screen.getByText('Needs b ^2, which is off or has another version.')).toBeInTheDocument();
    expect(screen.getAllByText('Starts after a restart').length).toBeGreaterThan(0);
    expect(screen.getAllByRole('button', { name: 'Open' })).toHaveLength(2);
  });

  it('notices: restart, errors, invalid files; phone layout', async () => {
    store.state = 'running';
    store.players = [{ name: 'a', ip: '', id: '1', joinedAt: 0 }];
    mods.error = 'Stop the server first.';
    mods.overview = overview({
      mods: [makeMod({ status: 'restart' })],
      running: true,
      changes: [{ kind: 'installed', id: 'land-claims', version: '1.4.2' }, { kind: 'removed', id: 'gone', version: '1' }],
      problems: [{ file: 'junk.bhmod', message: 'not a valid zip' }],
      error: { code: 'config', detail: 'bad' }
    });
    render(Mods);
    expect(screen.getByText('Restart the server to apply 2 changes')).toBeInTheDocument();
    expect(screen.getByText('Land Claims 1.4.2 was installed. gone was removed.')).toBeInTheDocument();
    expect(screen.getByText(/1 player is online/)).toBeInTheDocument();
    expect(screen.getByText('mods.toml can’t be read')).toBeInTheDocument();
    expect(screen.getByText('1 file in the mods folder is not a valid mod')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Restart when empty' }));
    expect(store.restartWhenEmpty).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    await fireEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(screen.queryByText('Restart the server to apply 2 changes')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mock.api.modsOverview).toHaveBeenCalled();
    mock.api.modsOverview = vi.fn(async () => mods.overview);
    await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(mock.api.modsRemove).toHaveBeenCalledWith('junk.bhmod');
    await waitFor(() => expect(screen.queryByText('Stop the server first.')).toBeNull());
    mods.error = 'x';
    await waitFor(() => expect(screen.getByText('x')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(mods.error).toBe('');
    cleanup();

    // A folder that can't be read, on Android (no folder button) and a phone.
    store.info = { version: '1', dataDir: '/d', platform: 'android', nativeControls: false };
    viewport.phone = true;
    mods.overview = null;
    mods.loadError = { code: 'io', detail: 'denied' };
    render(Mods);
    expect(screen.getByText('Couldn’t read the mods folder')).toBeInTheDocument();
    expect(screen.getByText('The file system refused: denied')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open folder' })).toBeNull();
    cleanup();
    mods.loadError = null;
    mods.overview = overview({ mods: demo(), clientMods: ['minimap'], error: { code: 'io', file: '/m', detail: 'x' } });
    render(Mods);
    expect(screen.getByText('Minimap runs on players’ devices.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Show client mods/ })).toBeNull();
    expect(screen.queryByLabelText('Sides')).toBeNull();
    expect(screen.getByText(/^\/m — /)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Add mod' }));
    expect(mods.view).toBe('browse');
  });
});

describe('Mod detail', () => {
  function open(id: string, over: Partial<ModsOverview> = {}) {
    mods.overview = overview({ mods: demo(), ...over });
    const keep = mods.overview;
    mock.api.modsOverview = vi.fn(async () => keep);
    mods.view = 'detail';
    mods.detailId = id;
    return render(Mods);
  }

  it('a healthy mod with an update, permissions and storage', async () => {
    store.state = 'stopped';
    const mods0 = demo();
    mods0[1] = makeMod({ id: 'land-claims', update: '1.5.0', storageBytes: 2200000, permissions: ['world:read', 'world:write'], granted: ['world:write'], lines: [], status: 'ready' });
    open('land-claims', { mods: mods0 });
    expect(screen.getByRole('heading', { name: /Land Claims/ })).toBeInTheDocument();
    expect(screen.getByText('Update 1.5.0 is available')).toBeInTheDocument();
    expect(screen.getByText('2.1 MB')).toBeInTheDocument();
    expect(screen.getByText('Not running, so there is nothing to measure.')).toBeInTheDocument();
    expect(screen.getByText('Start the server to see this mod’s log.')).toBeInTheDocument();
    expect(screen.getByText('Needs no other mods.')).toBeInTheDocument();
    // world:write covers world:read
    expect(screen.getAllByText('Granted')).toHaveLength(2);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Revoke' })[0]);
    expect(mock.api.modsSetGrant).toHaveBeenCalledWith('land-claims', 'world:read', false);
    await fireEvent.click(screen.getByRole('button', { name: 'Update 1.5.0' }));
    expect(mock.api.modsPlan).toHaveBeenCalledWith('land-claims', undefined);
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Open mod folder' }));
    expect(mock.api.modsOpenFolder).toHaveBeenCalledWith('land-claims');
    await fireEvent.click(screen.getByRole('button', { name: 'Reload mod' }));
    expect(mock.api.modsOverview).toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('switch', { name: 'On' }));
    await waitFor(() => expect(mock.api.modsSetEnabled).toHaveBeenCalledWith('land-claims', false));

    // Clear data and remove ask first.
    await fireEvent.click(screen.getByRole('button', { name: 'Clear data…' }));
    const d = screen.getByRole('dialog', { name: 'Clear the data of Land Claims?' });
    await fireEvent.click(within(d).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Clear data…' }));
    await fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Clear data' }));
    await waitFor(() => expect(mock.api.modsClearData).toHaveBeenCalledWith('land-claims'));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove mod' }));
    await fireEvent.click(within(screen.getByRole('dialog', { name: 'Remove Land Claims?' })).getByRole('button', { name: 'Remove mod' }));
    await waitFor(() => expect(mods.view).toBe('list'));
  });

  it('the admin overrides whether players need a client mod', async () => {
    store.state = 'stopped';
    open('minimap');
    const sec = screen.getByRole('region', { name: 'Players must have this mod' });
    expect(within(sec).getByText(/The mod’s own default is required/)).toBeInTheDocument();
    await fireEvent.click(within(sec).getByRole('switch', { name: 'required' }));
    await waitFor(() => expect(mock.api.modsSetClientRequired).toHaveBeenCalledWith('minimap', false));
    expect(toasts.items.at(-1)?.message).toBe('Saved. It applies after a restart.');
    cleanup();
    const locked = demo();
    locked[2] = makeMod({ id: 'minimap', sides: { server: true, client: true, clientRequired: true, clientRequiredDefault: false, clientRequiredLocked: true } });
    open('minimap', { mods: locked });
    expect(screen.getByText('This mod adds content to the world, so every player needs it.')).toBeInTheDocument();
    cleanup();
    const optional = demo();
    optional[2] = makeMod({ id: 'minimap', sides: { server: true, client: true, clientRequired: false, clientRequiredDefault: false, clientRequiredLocked: false } });
    open('minimap', { mods: optional });
    expect(screen.getByRole('switch', { name: 'optional' })).not.toBeChecked();
    mods.busy = 'minimap:clientRequired';
    await waitFor(() => expect(screen.getByRole('switch', { name: 'optional' })).toBeDisabled());
    mods.busy = '';
    expect(screen.getByText(/The mod’s own default is optional/)).toBeInTheDocument();
  });

  it('a mod over its time budget while running: meter, faults, log, console link', async () => {
    store.state = 'running';
    const w = makeMod({
      id: 'weather-plus',
      name: { en: 'Weather Plus' },
      description: {},
      status: 'budget',
      detail: '3 faults within 60 s',
      slowestMs: 7.9,
      faults: [
        { at: 1, kind: 'slow', text: 'tick took 7.9 ms', ms: 7.9 },
        { at: 2, kind: 'error', text: 'tick: timeout' }
      ],
      lines: [{ at: 1, level: 'warn', text: 'tick took 7.9 ms' }]
    });
    open('weather-plus', { mods: [w], running: true });
    expect(screen.getByText('Turned off automatically')).toBeInTheDocument();
    expect(screen.getByRole('meter', { name: 'Slowest call' })).toBeInTheDocument();
    expect(screen.getByText('Over budget. The bar turns red and striped.')).toBeInTheDocument();
    expect(screen.getByText('Fault: tick: timeout')).toBeInTheDocument();
    expect(screen.getByText('Slow call: tick took 7.9 ms')).toBeInTheDocument();
    expect(screen.getByRole('log', { name: 'Log lines from Weather Plus' })).toHaveTextContent(/warn\s+tick took 7\.9 ms/);
    expect(screen.getByText('Shown while the server is stopped')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy fault report' }));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalled());
    expect(toasts.items.at(-1)!.message).toBe('Copied');
    await fireEvent.click(screen.getByRole('button', { name: 'Turn on again' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    store.state = 'running';
    await fireEvent.click(screen.getByRole('button', { name: 'Reload mod' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalledTimes(2));
    await fireEvent.click(screen.getByRole('button', { name: 'Open in console' }));
    expect([store.screen, store.consoleQuery]).toEqual(['console', 'weather-plus:']);
  });

  it('running mods: measured, quiet; nothing logged yet', () => {
    store.state = 'running';
    open('better-chat', { mods: [makeMod({ id: 'better-chat', slowestMs: null, homepage: 'http://x.org/a' }), makeMod({ id: 'two', slowestMs: 1.2 })] });
    expect(screen.getByText('No slow calls in this run: every call stayed under 5 ms.')).toBeInTheDocument();
    expect(screen.getByText('Nothing logged yet.')).toBeInTheDocument();
    expect(screen.getByText('x.org/a')).toBeInTheDocument();
    cleanup();
    open('two', { mods: [makeMod({ id: 'two', slowestMs: 1.2, permissions: [] })] });
    expect(screen.getByText('The black line marks the budget.')).toBeInTheDocument();
    expect(screen.getByText('Asks for no permissions.')).toBeInTheDocument();
  });

  it('failed native mods, dependencies, permissions and restarts', async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => { throw new Error('no'); }) } });
    open('fast-tools');
    expect(screen.getByRole('group', { name: 'Danger: native code' })).toHaveTextContent('It isn’t on the public list.');
    expect(screen.getByText('Failed to start')).toBeInTheDocument();
    expect(screen.getByText('Native mods have no time or memory budget.')).toBeInTheDocument();
    expect(screen.getByText('Not from the public list')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Revoke (turns mod off)' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy error' }));
    await waitFor(() => expect(toasts.items.at(-1)!.message).toBe(t('qr.copyFailed')));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Reload mod' })[1]);
    cleanup();

    open('shop-signs');
    expect(screen.getByText('Waiting for economy-core')).toBeInTheDocument();
    expect(screen.getByText('(optional)')).toBeInTheDocument();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Install economy-core' })[0]);
    expect(mock.api.modsPlan).toHaveBeenCalledWith('economy-core', undefined);
    mods.flow = null;
    await fireEvent.click(screen.getAllByRole('button', { name: 'Install' })[0]);
    mods.flow = null;
    cleanup();

    open('x', { mods: [makeMod({ id: 'x', status: 'dependency', dependencies: [{ id: 'b', req: '1', installed: '2.0.0', ok: false, soft: false }, { id: 'c', req: '1', installed: '1.0.0', ok: true, soft: false }] })] });
    expect(screen.getByText('Off or 2.0.0')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install b' })).toBeNull();
    cleanup();

    open('discord-bridge');
    expect(screen.getByText('Waiting for your permission')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Grant' }));
    expect(mock.api.modsSetGrant).toHaveBeenCalledWith('discord-bridge', 'net-http:discord.com', true);
    cleanup();

    viewport.phone = true;
    open('discord-bridge');
    await fireEvent.click(screen.getByRole('switch', { name: 'Internet: discord.com: not granted' }));
    expect(mock.api.modsSetGrant).toHaveBeenCalledTimes(2);
    await fireEvent.click(screen.getByRole('switch', { name: 'Send and read chat: granted' }));
    expect(mock.api.modsSetGrant).toHaveBeenLastCalledWith('discord-bridge', 'players:chat', false);
    cleanup();

    viewport.phone = false;
    store.state = 'running';
    open('c', { mods: [makeMod({ id: 'c', status: 'restart', authors: [] })] });
    expect(screen.getByRole('button', { name: 'Clear data…' })).toBeDisabled();
    expect(screen.getByText('—')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    mods.error = 'Broken';
    await waitFor(() => expect(screen.getByText('Broken')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(mods.error).toBe('');
    await fireEvent.click(screen.getByRole('button', { name: 'All mods' }));
    expect(mods.view).toBe('list');
  });
});

describe('Browse', () => {
  function browse(list: BrowseMod[], over = {}) {
    mods.view = 'browse';
    mods.browse = { generated: '2026-10-21T12:00:00Z', offline: false, error: null, hidden: 0, mods: list, ...over };
    return render(Mods);
  }

  it('cards with sides, permissions, dependencies, native band and install buttons', async () => {
    store.now = Date.parse('2026-10-22');
    mods.overview = overview({ mods: [makeMod({ id: 'base' })] });
    browse([
      browseMod({ depends: { 'economy-core': '^2', base: '1' }, id: 'auction', name: 'Auction House', permissions: ['players:inventory'] }),
      browseMod({ id: 'turbo', name: 'Turbo', native: true, clientRequired: true, permissions: ['native'], tags: [], size: null, published: '' }),
      browseMod({ id: 'claims', name: 'Land Claims', installed: '1.0.0', version: '1.5.0', owners: ['tobyblock'] }),
      browseMod({ id: 'chat', name: 'Better Chat', installed: '2.3.1' })
    ]);
    expect(screen.getByText('4 of 4 mods')).toBeInTheDocument();
    expect(screen.getAllByTestId('browse-card')).toHaveLength(4);
    expect(screen.getByText(/Needs economy-core \(installed for you\), base/)).toBeInTheDocument();
    expect(screen.getByText('Native code · not sandboxed')).toBeInTheDocument();
    expect(screen.getByText('Players need the mod loader')).toBeInTheDocument();
    expect(screen.getAllByText('Updated 3 days ago').length).toBeGreaterThan(0);
    expect(screen.getByText('Installed')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Update' }));
    expect(mock.api.modsPlan).toHaveBeenCalledWith('claims', undefined);
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Install Turbo' }));
    expect(mock.api.modsPlan).toHaveBeenLastCalledWith('turbo', undefined);
    mods.flow = null;

    await fireEvent.click(screen.getByRole('button', { name: /Server only/ }));
    expect(screen.getAllByTestId('browse-card')).toHaveLength(3);
    await fireEvent.click(screen.getByRole('button', { name: /Needs client/ }));
    expect(screen.getAllByTestId('browse-card')).toHaveLength(1);
    await fireEvent.click(screen.getByRole('switch', { name: 'Hide native mods' }));
    expect(screen.getByTestId('browse-none')).toHaveTextContent('No mods match these filters');
    await fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(screen.getAllByTestId('browse-card')).toHaveLength(4);
    await fireEvent.click(screen.getByRole('button', { name: 'economy' }));
    expect(screen.getAllByTestId('browse-card')).toHaveLength(3);
    await fireEvent.click(screen.getByRole('button', { name: 'All' }));
    await fireEvent.change(screen.getByLabelText('Sort'), { target: { value: 'name' } });
    expect(screen.getAllByTestId('browse-card')[0]).toHaveTextContent('Auction House');
    await fireEvent.input(screen.getByLabelText('Search mods, authors, tags'), { target: { value: 'jetpack' } });
    expect(screen.getByText('No mods found for “jetpack”')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Install from file or link' }));
    expect(mods.flow!.stage).toBe('file');
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Mods' }));
    expect(mods.view).toBe('list');
  });

  it('loading, offline, cached and empty lists', async () => {
    viewport.phone = true;
    mods.view = 'browse';
    mods.browseLoading = true;
    render(Mods);
    expect(screen.getByText('Loading the mod list…')).toBeInTheDocument();
    mods.browseLoading = false;
    mods.browseError = { code: 'offline', detail: 'dns' };
    await waitFor(() => expect(screen.getByText('Can’t reach blockheads.club')).toBeInTheDocument());
    expect(screen.getByText('The mod list can’t be reached: dns')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Install from file' }));
    expect(mods.flow!.stage).toBe('file');
    mods.flow = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(screen.getByText('The mod list is empty for now')).toBeInTheDocument());
    cleanup();
    browse([browseMod()], { offline: true });
    expect(screen.getByText('blockheads.club can’t be reached. This list is from 2026-10-21.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mock.api.modsBrowse).toHaveBeenCalledTimes(2);
  });
});

describe('Install flow dialog', () => {
  function flowWith(over: Partial<NonNullable<typeof mods.flow>> = {}) {
    mods.overview = overview();
    mods.flow = { stage: 'plan', origin: { kind: 'index', id: 'auction-house' }, plan: plan(), grants: {}, trust: false, enable: true, progress: null, error: null, result: null, ...over };
    return render(Mods);
  }

  it('plan step with dependencies, already installed and missing mods', async () => {
    flowWith({
      plan: plan({
        items: [item({ replaces: '1.0.0' }), item({ id: 'eco', name: 'Eco', neededBy: 'auction-house', reviewed: false, native: true, sides: ['server', 'client'], clientRequired: true, size: null })],
        satisfied: [{ id: 'base', req: '^1', version: '1.4.0' }],
        totalBytes: 400000
      })
    });
    const d = screen.getByRole('dialog', { name: 'Install Auction House' });
    expect(within(d).getByText('Step 1 of 2 · What gets installed')).toBeInTheDocument();
    expect(within(d).getByText('Auction House needs one more mod. We add it for you.')).toBeInTheDocument();
    expect(within(d).getByText('Needed by auction-house')).toBeInTheDocument();
    expect(within(d).getByText('Replaces 1.0.0')).toBeInTheDocument();
    expect(within(d).getByText('Already installed (1.4.0)')).toBeInTheDocument();
    expect(within(d).getByText('Also runs on players’ devices')).toBeInTheDocument();
    expect(within(d).getByText('Total download 391 kB')).toBeInTheDocument();
    await fireEvent.click(within(d).getByRole('button', { name: 'Continue' }));
    expect(within(d).getByText('Step 2 of 2 · Permissions')).toBeInTheDocument();
    expect(within(d).getByText('Native code: you confirm it on the next step.')).toBeInTheDocument();
    await fireEvent.click(within(d).getByRole('button', { name: 'Back' }));
    expect(mods.flow!.stage).toBe('plan');
    await fireEvent.click(within(d).getByRole('button', { name: 'Cancel' }));
    expect(mods.flow).toBeNull();
    cleanup();

    flowWith({ plan: plan({ items: [item()], missing: [{ id: 'ghost', req: '1', neededBy: 'auction-house' }] }) });
    expect(screen.getByText('A needed mod can’t be found')).toBeInTheDocument();
    expect(screen.getByText('Not on the list')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
    cleanup();
    flowWith({ plan: plan({ items: [item()], satisfied: [{ id: 'base', req: '1', version: '1' }] }) });
    expect(screen.getByText('All server-only')).toBeInTheDocument();
  });

  it('permission step: ticks, keep off, turn on', async () => {
    mock.api.modsInstall = vi.fn(async () => ({ ids: ['auction-house'], restart: false }));
    flowWith({ stage: 'perms', grants: { 'auction-house': ['players:inventory', 'players:chat'] }, plan: plan({ items: [item(), item({ id: 'quiet', name: 'Quiet', permissions: [] })] }) });
    const d = screen.getByRole('dialog', { name: 'Choose what these mods may do' });
    expect(within(d).getByText('Asks for no permissions.')).toBeInTheDocument();
    const boxes = within(d).getAllByRole('checkbox');
    expect(boxes.every((b) => (b as HTMLInputElement).checked)).toBe(true);
    await fireEvent.click(boxes[1]);
    expect(mods.flow!.grants['auction-house']).toEqual(['players:inventory']);
    await fireEvent.click(boxes[1]);
    await fireEvent.click(within(d).getByRole('button', { name: 'Install, keep off' }));
    await waitFor(() => expect(screen.getByText('It is installed and stays off until you turn it on.')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Open Auction House' }));
    expect([mods.flow, mods.detailId]).toEqual([null, 'auction-house']);
    cleanup();
    flowWith({ stage: 'perms' });
    expect(screen.getByText('Permissions')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Install and turn on' }));
    await waitFor(() => expect(screen.getByText('It starts with the server.')).toBeInTheDocument());
    await fireEvent.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!);
    expect(mods.flow).toBeNull();
    cleanup();
    flowWith({ stage: 'perms' });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mods.flow).toBeNull();
  });

  it('native confirmation needs the box ticked and starts on the safe choice', async () => {
    flowWith({ stage: 'perms', plan: plan({ items: [item({ native: true, permissions: ['native'], name: 'Turbo', clientRequired: true, reviewed: false })] }) });
    await fireEvent.click(screen.getByRole('button', { name: 'Install and turn on' }));
    const d = screen.getByRole('dialog', { name: 'Install Turbo?' });
    expect(within(d).getByRole('group', { name: 'Danger: native code' })).toHaveTextContent('nobody at Blockheads Club has reviewed it');
    expect(within(d).getByText('Also runs on players’ devices')).toBeInTheDocument();
    await waitFor(() => expect(within(d).getByRole('button', { name: 'Don’t install' })).toHaveFocus());
    expect(within(d).getByRole('button', { name: 'Install native mod' })).toBeDisabled();
    expect(within(d).getByText('Tick the box to continue.')).toBeInTheDocument();
    await fireEvent.click(within(d).getByRole('checkbox'));
    await fireEvent.click(within(d).getByRole('button', { name: 'Install native mod' }));
    await waitFor(() => expect(mods.flow!.stage).toBe('done'));
    expect(mock.api.modsInstall.mock.calls[0][0].grants).toEqual({ 'auction-house': ['native'] });
    cleanup();
    flowWith({ stage: 'native', plan: plan({ items: [item({ native: true, permissions: ['native'] })] }) });
    expect(screen.getByText(/The maintainers reviewed it/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Don’t install' }));
    expect(mods.flow).toBeNull();
  });

  it('installing, done with a restart, failures', async () => {
    store.state = 'running';
    flowWith({ stage: 'installing', plan: plan({ items: [item(), item({ id: 'b' })] }) });
    expect(screen.getByRole('dialog', { name: 'Installing 2 mods…' })).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
    mods.flow!.progress = { step: 'download', done: 1, total: 2 };
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '45'));
    expect(screen.getByText('Downloading and checking 2 of 2')).toBeInTheDocument();
    mods.flow!.progress = { step: 'write', done: 2, total: 2 };
    await waitFor(() => expect(screen.getByText('Writing the files')).toBeInTheDocument());
    mods.flow!.progress = { step: 'done', done: 2, total: 2 };
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100'));
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(mods.flow).not.toBeNull();
    mods.flow!.result = { ids: ['auction-house'], restart: true };
    mods.flow!.stage = 'done';
    await waitFor(() => expect(screen.getByText('Restart needed')).toBeInTheDocument());
    expect(screen.getByText(/Nobody is online right now/)).toBeInTheDocument();
    store.players = [{ name: 'a', ip: '', id: '1', joinedAt: 0 }];
    await waitFor(() => expect(screen.getByText(/1 player is online/)).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Restart when empty' }));
    expect([store.restartWhenEmpty, mods.flow]).toEqual([true, null]);
    cleanup();
    flowWith({ stage: 'done', result: { ids: [], restart: true } });
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    cleanup();

    for (const [code, title] of [
      ['checksum', 'The file check failed'],
      ['download', 'Couldn’t download the mod'],
      ['invalid', 'This isn’t a valid mod'],
      ['io', 'Couldn’t install']
    ]) {
      flowWith({ stage: 'failed', error: { code, file: 'a.bhmod', expected: 'aa', got: 'bb' } });
      expect(screen.getByRole('dialog', { name: title })).toBeInTheDocument();
      cleanup();
    }
    flowWith({ stage: 'failed', origin: { kind: 'local' }, error: { code: 'checksum', file: 'a.bhmod', expected: 'aa', got: 'bb' } });
    expect(screen.getByText(/expected\s+aa/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy details' }));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith(expect.stringContaining('got  bb')));
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mods.flow!.stage).toBe('file');
    mods.flow!.stage = 'failed';
    mods.flow!.error = { code: 'other' };
    await waitFor(() => expect(screen.queryByText(/expected/)).toBeNull());
    await fireEvent.click(screen.getByRole('button', { name: 'Choose another file' }));
    expect(mods.flow!.stage).toBe('file');
  });

  it('file step: drop, choose, link; checking', async () => {
    flowWith({ stage: 'file', origin: { kind: 'local' }, plan: null });
    const d = screen.getByRole('dialog', { name: 'Install from a file or link' });
    expect(within(d).getByText('Not from the public list')).toBeInTheDocument();
    expect(within(d).getByRole('button', { name: 'Continue' })).toBeDisabled();
    const zone = screen.getByTestId('drop');
    await fireEvent.dragOver(zone);
    expect(zone).toHaveClass('over');
    await fireEvent.dragLeave(zone);
    expect(zone).not.toHaveClass('over');
    await fireEvent.drop(zone, { dataTransfer: { files: [] } });
    expect(mock.api.modsInspectFile).not.toHaveBeenCalled();
    const file = new File([new Uint8Array([1])], 'a.bhmod');
    await fireEvent.drop(zone, { dataTransfer: { files: [file] } });
    await waitFor(() => expect(mock.api.modsInspectFile).toHaveBeenCalledWith('a.bhmod', new Uint8Array([1])));
    cleanup();

    flowWith({ stage: 'file', origin: { kind: 'local' }, plan: null });
    const input = screen.getByLabelText('Choose file') as HTMLInputElement;
    await fireEvent.change(input);
    expect(mock.api.modsInspectFile).toHaveBeenCalledTimes(1);
    Object.defineProperty(input, 'files', { value: [file] });
    await fireEvent.change(input);
    await waitFor(() => expect(mock.api.modsInspectFile).toHaveBeenCalledTimes(2));
    cleanup();

    let release: () => void = () => {};
    mock.api.modsInspectUrl = vi.fn(() => new Promise((r) => (release = () => r({ source: 'a', sha256: 'f', plan: plan() }))));
    flowWith({ stage: 'file', origin: { kind: 'local' }, plan: null });
    await fireEvent.input(screen.getByLabelText('Or paste a link'), { target: { value: 'https://x/a.bhmod' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByRole('dialog', { name: 'Checking the mod…' })).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mods.flow).toBeNull();
    release();
    cleanup();
    flowWith({ stage: 'file', origin: { kind: 'local' }, plan: null });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mods.flow).toBeNull();
  });
});

describe('Dashboard card', () => {
  it('summarises the mods and links to them', async () => {
    render(ModsCard);
    expect(screen.getByText('No mods installed')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Browse mod list' }));
    expect(mods.view).toBe('browse');
    cleanup();
    mods.overview = overview({ mods: demo(), clientMods: ['minimap'] });
    render(ModsCard);
    expect(screen.getByText('3 of 8 running · players need the mod loader')).toBeInTheDocument();
    expect(screen.getByText('2 failed or turned off')).toBeInTheDocument();
    expect(screen.getByText('2 waiting')).toBeInTheDocument();
    expect(screen.getByText('1 update')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Manage mods' }));
    expect(mods.view).toBe('list');
    cleanup();
    mods.overview = overview({ mods: [makeMod()] });
    render(ModsCard);
    expect(screen.getByText('1 of 1 running')).toBeInTheDocument();
  });
});

describe('app store hooks', () => {
  it('flags mod changes from the log and restarts when the server is empty', async () => {
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', lines: ['x [mods] a: enabled'], times: [1] }));
    store.state = 'running';
    const before = store.modsDirty;
    store.restartWhenEmpty = true;
    await store.pollOnce();
    expect(store.modsDirty).toBe(before + 1);
    expect(store.restartWhenEmpty).toBe(false);
    await waitFor(() => expect(mock.api.stopServer).toHaveBeenCalled());
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', players: [{ name: 'a', ip: '', id: '1', joinedAt: 0 }] }));
    store.state = 'running';
    store.restartWhenEmpty = true;
    const now = store.modsDirty;
    await store.pollOnce();
    expect([store.restartWhenEmpty, store.modsDirty]).toEqual([true, now]);
  });
});

describe('updates and edge cases', () => {
  it('sidebar shows how many mods need attention', async () => {
    const Sidebar = (await import('../layout/Sidebar.svelte')).default;
    mods.overview = overview({ mods: demo() });
    render(Sidebar);
    expect(screen.getByLabelText('4 mods need attention')).toHaveTextContent('4');
  });

  it('the meter follows new values', async () => {
    const r = render(BudgetMeter, { usedMs: 1, budgetMs: 5, maxMs: 25, label: 'm' });
    await r.rerender({ usedMs: 9, budgetMs: 4, maxMs: 25, label: 'm' });
    expect(screen.getByRole('meter').querySelector('.over')).not.toBeNull();
  });

  it('the detail page follows another mod and the language', async () => {
    const a = makeMod({ id: 'a', name: { en: 'Alpha' }, status: 'budget', detail: null, native: true, reviewed: false, homepage: null, runtime: null, dependencies: [{ id: 'x', req: '1', installed: null, ok: false, soft: true }], lines: [{ at: 1, level: 'info', text: 'one' }], permissions: ['players:kick'], granted: [] });
    const b = makeMod({ id: 'b', name: { en: 'Beta' }, status: 'failed', detail: null, native: true, reviewed: true, enabled: false, dependencies: [{ id: 'x', req: '1', installed: null, ok: false, soft: false }], lines: [{ at: 2, level: 'error', text: 'two' }], permissions: ['players:ban'], granted: ['players:ban'] });
    mods.overview = overview({ mods: [a, b] });
    mods.view = 'detail';
    mods.detailId = 'a';
    render(Mods);
    expect(screen.getByRole('heading', { name: /Alpha/ })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy fault report' }));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith('a 1.4.2: budget'));
    mods.detailId = 'b';
    await waitFor(() => expect(screen.getByRole('heading', { name: /Beta/ })).toBeInTheDocument());
    expect(screen.getByRole('switch', { name: 'Off' })).toBeInTheDocument();
    setLang('tr');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Modu kaldır' })).toBeInTheDocument());
    setLang('en');
    // The remove question closes with Escape.
    await fireEvent.click(await screen.findByRole('button', { name: 'Remove mod' }));
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    cleanup();
    store.info = { version: '1', dataDir: '/d', platform: 'android', nativeControls: false };
    mods.detailId = 'a';
    render(Mods);
    expect(screen.queryByRole('button', { name: 'Open mod folder' })).toBeNull();
  });

  it('rows open by name; missing permissions without a host', async () => {
    mods.overview = overview({ mods: [makeMod({ id: 'k', name: { en: 'Kicker' }, status: 'permissions', permissions: ['players:kick'], missing: ['players:kick'] })] });
    render(Mods);
    expect(screen.getByText('Asks for Kick players. Not granted yet.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Kicker' }));
    expect(mods.view).toBe('detail');
  });

  it('the list follows changes: players, problems, language; Android footer; folder from the error', async () => {
    store.state = 'running';
    mods.overview = overview({ mods: [makeMod()], running: true, changes: [{ kind: 'updated', id: 'x', version: '2' }], problems: [{ file: 'a', message: 'one' }], error: { code: 'io' } });
    render(Mods);
    store.players = [{ name: 'a', ip: '', id: '1', joinedAt: 0 }];
    await waitFor(() => expect(screen.getByText(/1 player is online/)).toBeInTheDocument());
    mods.overview = { ...mods.overview!, problems: [{ file: 'a', message: 'two' }] };
    await waitFor(() => expect(screen.getByText(/two/)).toBeInTheDocument());
    setLang('ru');
    await waitFor(() => expect(screen.getByRole('option', { name: 'Все стороны' })).toBeInTheDocument());
    setLang('en');
    await fireEvent.click(await screen.findByRole('button', { name: 'Open folder' }));
    expect(mock.api.modsOpenFolder).toHaveBeenCalled();
    cleanup();
    // No overview at all and an error without a file: no path.
    mods.overview = null;
    mods.loadError = { code: 'io', detail: 'gone' };
    render(Mods);
    expect(screen.getByText('The file system refused: gone')).toBeInTheDocument();
    cleanup();
    mods.loadError = null;
    store.info = { version: '1', dataDir: '/d', platform: 'android', nativeControls: false };
    mods.overview = overview({ mods: [makeMod()] });
    render(Mods);
    expect(screen.getByText('/data/mods')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open folder' })).toBeNull();
  });

  it('browse follows changes; nothing loaded yet shows nothing', async () => {
    mods.view = 'browse';
    render(Mods);
    expect(screen.queryByTestId('browse-card')).toBeNull();
    mods.browse = { generated: '', offline: false, error: null, hidden: 0, mods: [browseMod({ description: null, permissions: ['players:kick'] })] };
    await waitFor(() => expect(screen.getAllByTestId('browse-card')).toHaveLength(1));
    mods.browse = { ...mods.browse, mods: [browseMod({ clientRequired: true })] };
    await waitFor(() => expect(screen.getByText('Players need the mod loader')).toBeInTheDocument());
    setLang('kk');
    await waitFor(() => expect(screen.getByRole('option', { name: 'Атауы' })).toBeInTheDocument());
    setLang('en');
    mods.bquery = 'coins';
    expect(mods.browseList).toHaveLength(1);
    mods.browse = { ...mods.browse, mods: [browseMod({ description: null })] };
    expect(mods.browseList).toHaveLength(0);
  });

  it('install dialog: kept on screen while closing, copy failures, risky permissions, cancelled checks', async () => {
    const InstallFlow = (await import('./InstallFlow.svelte')).default;
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => { throw new Error('no'); }) } });
    mods.flow = { stage: 'failed', origin: { kind: 'index', id: 'a' }, plan: null, grants: {}, trust: false, enable: true, progress: null, error: { code: 'io' }, result: null };
    render(InstallFlow);
    await fireEvent.click(screen.getByRole('button', { name: 'Copy details' }));
    await waitFor(() => expect(toasts.items.at(-1)!.message).toBe(t('qr.copyFailed')));
    mods.flow = null;
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    cleanup();
    mods.flow = { stage: 'perms', origin: { kind: 'index', id: 'a' }, plan: plan({ items: [item({ permissions: ['players:kick'] })] }), grants: {}, trust: false, enable: true, progress: null, error: null, result: null };
    render(InstallFlow);
    expect(screen.getByText('Kick players')).toBeInTheDocument();
    mods.flow.plan = plan({ items: [item({ permissions: ['players:kick'], native: true })] });
    await waitFor(() => expect(screen.getByText('Native code: you confirm it on the next step.')).toBeInTheDocument());
    mods.flow.stage = 'installing';
    mods.flow.progress = { step: 'download', done: 0, total: 1 };
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0'));
    mods.flow.progress = { step: 'write', done: 1, total: 1 };
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '90'));
    cleanup();

    // A check whose dialog was closed meanwhile neither reopens nor fails it.
    let fail: (e: unknown) => void = () => {};
    mock.api.modsPlan = vi.fn(() => new Promise((_r, j) => (fail = j)));
    const p = mods.install('x');
    mods.flow = null;
    fail({ code: 'download' });
    await p;
    expect(mods.flow).toBeNull();
  });
});
