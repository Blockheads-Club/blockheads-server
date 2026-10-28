import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('../api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from '../i18n.svelte';
import { makeApi, makeMod, makeOverview } from '../mock-api';
import Mods from '../screens/Mods.svelte';
import { store } from '../store.svelte';
import type { Json, ModSettings, SettingField } from '../types';
import { toasts } from '../ui/toast.svelte';
import { mods } from './mods.svelte';
import SettingControl from './SettingControl.svelte';
import {
  clone,
  fieldText,
  itemFields,
  matrixRoles,
  modSettings,
  newItem,
  problemText,
  roleName,
  roleOptions,
  same,
  setCell,
  settingErr,
  validate
} from './settings.svelte';

const f = (over: Partial<SettingField> & Pick<SettingField, 'key' | 'type'>): SettingField => ({
  default: null,
  label: { en: over.key },
  scope: 'server',
  restart: false,
  ...over
});

/** The voice plan's settings, one field of each kind. */
function schema(): ModSettings {
  return {
    fields: [
      f({ key: 'proximity', type: 'bool', default: true, label: { en: 'Proximity chat', tr: 'Yakınlık sohbeti' }, help: { en: 'Hear players nearby' }, group: 'Modes' }),
      f({ key: 'radius', type: 'int', default: 32, min: 4, max: 128, step: 4, restart: true, group: 'Proximity', label: { en: 'Radius' } }),
      f({ key: 'muffle', type: 'float', default: 0.15, min: 0, group: 'Proximity', label: { en: 'Muffle' } }),
      f({ key: 'gain', type: 'float', default: 1, max: 2, group: 'Proximity', label: { en: 'Gain' } }),
      f({ key: 'curve', type: 'enum', default: 'log', values: ['linear', 'log'], group: 'Proximity', label: { en: 'Curve' } }),
      f({ key: 'motd', type: 'string', default: '', max_length: 5, label: { en: 'Message' } }),
      f({ key: 'admins', type: 'roles', default: ['admin'], label: { en: 'Who may mute' } }),
      f({ key: 'talk', type: 'matrix', default: { proximity: { everyone: true } }, rows: ['proximity', 'global'], cols: ['owner', 'everyone'], cell: 'bool', label: { en: 'Who may talk' } }),
      f({ key: 'who', type: 'matrix', default: {}, rows: ['global'], cols: ['talk'], cell: 'roles', label: { en: 'Roles per mode' } }),
      f({
        key: 'channels',
        type: 'list',
        default: [],
        max_items: 2,
        label: { en: 'Channels' },
        item: {
          name: { type: 'string', default: '', max_length: 8, label: { en: 'Name' }, scope: 'server', restart: false },
          size: { type: 'int', default: 1, label: { en: 'Size' }, scope: 'server', restart: false }
        }
      }),
      f({ key: 'keys', type: 'string', default: 'V', scope: 'client' })
    ],
    values: { radius: 64, admins: ['admin', 'vip'] }
  };
}

function reset(over: Record<string, unknown> = {}) {
  mock.api = makeApi(over);
  setLang('en');
  toasts.items = [];
  store.state = 'stopped';
  store.players = [];
  store.screen = 'mods';
  Object.assign(modSettings, { id: null, data: null, loading: false, loadError: null, saved: {}, draft: {}, errors: {}, saving: false, restartNeeded: false });
  Object.assign(mods, { overview: null, loadError: null, error: '', view: 'list', detailId: null, tab: 'overview', busy: '' });
}

vi.setConfig({ testTimeout: 30000 });
beforeEach(() => reset());
afterEach(() => cleanup());

describe('settings helpers', () => {
  it('compare, copy and name values', () => {
    expect(same({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] })).toBe(true);
    expect(same({ a: 1 }, { b: 1 })).toBe(false);
    expect(same({ a: 1 }, { a: 1, b: 2 })).toBe(false);
    expect(same([1], { 0: 1 })).toBe(false);
    expect(same(null, {})).toBe(false);
    expect(same(1, 2)).toBe(false);
    const v = { a: [1] };
    expect(clone(v)).toEqual(v);
    expect(clone(v)).not.toBe(v);
    expect(fieldText(undefined, 'key')).toBe('key');
    expect(fieldText({ tr: 'Yalnız' }, 'key')).toBe('Yalnız');
    expect(roleOptions(['vip', 1, 'admin'], 'x', null)).toEqual(['owner', 'admin', 'mod', 'everyone', 'vip']);
    expect(roleName('mod')).toBe('Moderators');
    expect(roleName('vip')).toBe('vip');
    expect(matrixRoles({ a: { b: ['x'] }, c: 1, d: [] })).toEqual([['x']]);
    expect(matrixRoles([])).toEqual([]);
    expect(matrixRoles(null)).toEqual([]);
    expect(setCell(null, 'r', 'c', true)).toEqual({ r: { c: true } });
    expect(setCell({ r: { c: true, d: true } }, 'r', 'c', false)).toEqual({ r: { d: true } });
    expect(setCell({ r: { c: ['a'] } }, 'r', 'c', [])).toEqual({});
    const list = schema().fields[9];
    expect(itemFields(list).map((s) => s.key)).toEqual(['name', 'size']);
    expect(itemFields(f({ key: 'x', type: 'list' }))).toEqual([]);
    expect(newItem(list)).toEqual({ name: '', size: 1 });
  });

  it('validates like the backend', () => {
    const [bool, int, float, , en, str, roles, mtx, rmtx, list] = schema().fields;
    expect(validate(bool, true)).toBeNull();
    expect(validate(bool, 1)).toEqual({ rule: 'type', at: '' });
    expect(validate(int, 'x')).toEqual({ rule: 'number', at: '' });
    expect(validate(int, Infinity)?.rule).toBe('number');
    expect(validate(int, 4.5)?.rule).toBe('integer');
    expect(validate(int, 0)).toEqual({ rule: 'min', expected: '4', at: '' });
    expect(validate(int, 200)).toEqual({ rule: 'max', expected: '128', at: '' });
    expect(validate(int, 10)).toEqual({ rule: 'step', expected: '4', at: '' });
    expect(validate(int, 12)).toBeNull();
    expect(validate(f({ key: 's', type: 'int', step: 5 }), 10)).toBeNull();
    expect(validate(f({ key: 's', type: 'int', step: 0 }), 3)).toBeNull();
    expect(validate(float, -1)?.rule).toBe('min');
    expect(validate(float, '1')?.rule).toBe('number');
    expect(validate(float, 0.5)).toBeNull();
    expect(validate(en, 'cube')).toEqual({ rule: 'choice', expected: 'linear, log', at: '' });
    expect(validate(f({ key: 'e', type: 'enum' }), 'x')?.expected).toBe('');
    expect(validate(en, 'log')).toBeNull();
    expect(validate(str, 1)?.rule).toBe('type');
    expect(validate(str, 'toolong')).toEqual({ rule: 'length', expected: '5', at: '' });
    expect(validate(f({ key: 's', type: 'string' }), 'anything')).toBeNull();
    expect(validate(roles, 'admin')?.rule).toBe('type');
    expect(validate(roles, [' '])?.rule).toBe('role');
    expect(validate(roles, ['x'.repeat(41)])?.rule).toBe('role');
    expect(validate(roles, ['vip'])).toBeNull();
    expect(validate(mtx, [])?.rule).toBe('type');
    expect(validate(mtx, { x: {} })).toEqual({ rule: 'unknown', at: 'x' });
    expect(validate(mtx, { global: 1 })).toEqual({ rule: 'type', at: 'global' });
    expect(validate(mtx, { global: { x: true } })).toEqual({ rule: 'unknown', at: 'global.x' });
    expect(validate(mtx, { global: { owner: 1 } })).toEqual({ rule: 'type', at: 'global.owner' });
    expect(validate(mtx, { global: { owner: true } })).toBeNull();
    expect(validate(rmtx, { global: { talk: [''] } })).toEqual({ rule: 'role', at: 'global.talk' });
    expect(validate(f({ key: 'm', type: 'matrix' }), { a: {} })?.rule).toBe('unknown');
    expect(validate(f({ key: 'm', type: 'matrix', rows: ['a'] }), { a: { b: true } })?.rule).toBe('unknown');
    expect(validate(list, {})?.rule).toBe('type');
    expect(validate(list, [{}, {}, {}])).toEqual({ rule: 'items', expected: '2', at: '' });
    expect(validate(list, [1])).toEqual({ rule: 'type', at: 'channels[0]' });
    expect(validate(list, [{ x: 1 }])).toEqual({ rule: 'unknown', at: 'channels[0].x' });
    expect(validate(list, [{ name: 'a' }, { name: 'much too long' }])).toEqual({ rule: 'length', expected: '8', at: 'channels[1].name' });
    expect(validate(list, [{ name: 'a', size: 2 }], 'outer')).toBeNull();
    expect(validate(list, [{ size: 'x' }], 'outer')).toEqual({ rule: 'number', at: 'outer[0].size' });
  });

  it('turns problems and backend errors into text', () => {
    expect(problemText({ rule: 'min', expected: '4' })).toBe('Must be at least 4.');
    expect(problemText({ rule: 'length', expected: '8', at: 'a[0].name' })).toBe('a[0].name: At most 8 characters.');
    expect(problemText({ rule: 'length', expected: '8', at: 'a[0].name' }, true)).toBe('At most 8 characters.');
    expect(problemText({ rule: 'weird' })).toBe('This value isn’t allowed.');
    expect(problemText({ rule: 'type' })).toBe('This value has the wrong type.');
    expect(settingErr({ code: 'setting', detail: 'max', expected: '9' })).toBe('Must be at most 9.');
    expect(settingErr({ code: 'setting' })).toBe('This value isn’t allowed.');
    expect(settingErr({ code: 'settingKey', detail: 'x' })).toBe('The mod has no setting “x” you can change.');
    expect(settingErr('boom')).toBe('boom');
  });
});

describe('settings store', () => {
  it('loads, edits, saves and reports refusals', async () => {
    const set = vi.fn(async (_id: string, key: string) => {
      if (key === 'motd') throw { code: 'setting', detail: 'length', expected: '3' };
    });
    reset({ modSettingsGet: vi.fn(async () => schema()), modSettingsSet: set });
    store.state = 'running';
    await modSettings.load('voice');
    expect(modSettings.fields).toHaveLength(10);
    expect(modSettings.groups.map((g) => g.name)).toEqual(['Modes', 'Proximity', '']);
    expect(modSettings.saved.radius).toBe(64);
    expect(modSettings.saved.curve).toBe('log');
    expect(modSettings.dirty).toEqual([]);
    expect(modSettings.canSave).toBe(false);
    modSettings.set('radius', 10);
    expect(modSettings.problems.radius?.rule).toBe('step');
    expect(modSettings.canSave).toBe(false);
    modSettings.set('radius', 12);
    modSettings.set('motd', 'hello');
    expect(modSettings.canSave).toBe(true);
    await modSettings.save();
    expect(set).toHaveBeenCalledWith('voice', 'radius', 12);
    expect(modSettings.saved.radius).toBe(12);
    expect(modSettings.errors.motd).toBe('At most 3 characters.');
    expect(modSettings.restartNeeded).toBe(true);
    expect(toasts.items.at(-1)?.message).toBe('Some settings were not saved');
    expect(modSettings.dirty).toEqual(['motd']);
    modSettings.discard();
    expect(modSettings.dirty).toEqual([]);
    expect(modSettings.errors).toEqual({});
    modSettings.resetAll();
    expect(modSettings.draft.radius).toBe(32);
    expect(modSettings.dirty).toEqual(['radius', 'admins']);
    set.mockClear();
    store.state = 'stopped';
    modSettings.restartNeeded = false;
    await modSettings.save();
    expect(set).toHaveBeenCalledTimes(2);
    expect(modSettings.restartNeeded).toBe(false);
    expect(toasts.items.at(-1)?.message).toBe('Settings saved');
    // Nothing to save: no toast.
    toasts.items = [];
    await modSettings.save();
    expect(toasts.items).toEqual([]);
  });

  it('load failures and stale answers', async () => {
    reset({ modSettingsGet: vi.fn(async (id: string) => (id === 'bad' ? Promise.reject({ code: 'settingsSchema', detail: 'a: unknown type' }) : schema())) });
    await modSettings.load('bad');
    expect(modSettings.loadError).toEqual({ code: 'settingsSchema', detail: 'a: unknown type' });
    mock.api.modSettingsGet = vi.fn(async () => Promise.reject('down'));
    await modSettings.load('bad');
    expect(modSettings.loadError).toEqual({ code: 'other', detail: 'down' });
    // An answer for a mod the admin already left is dropped.
    let done!: (v: ModSettings) => void;
    let fail!: (e: unknown) => void;
    mock.api.modSettingsGet = vi.fn(() => new Promise<ModSettings>((r) => (done = r)));
    const first = modSettings.load('a');
    mock.api.modSettingsGet = vi.fn(() => new Promise<ModSettings>((_, j) => (fail = j)));
    const second = modSettings.load('b');
    mock.api.modSettingsGet = vi.fn(async () => schema());
    await modSettings.load('c');
    done(schema());
    fail('late');
    await Promise.all([first, second]);
    expect(modSettings.id).toBe('c');
    expect(modSettings.loadError).toBeNull();
  });
});

describe('Settings tab', () => {
  function open(settings = 10, over: Record<string, unknown> = {}) {
    const mod = makeMod({ id: 'voice', name: { en: 'Voice' }, settings });
    mods.overview = makeOverview({ mods: [mod] });
    const keep = mods.overview;
    reset({ modsOverview: vi.fn(async () => keep), modSettingsGet: vi.fn(async () => schema()), ...over });
    mods.overview = keep;
    mods.view = 'detail';
    mods.detailId = 'voice';
    return render(Mods);
  }

  it('renders every field type, edits, resets and saves', async () => {
    open();
    expect(screen.getByRole('tablist', { name: 'Mod sections' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    await screen.findByRole('heading', { name: 'Modes' });
    expect(screen.getByRole('heading', { name: 'Proximity' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'General' })).toBeInTheDocument();
    expect(screen.getByText('Hear players nearby')).toBeInTheDocument();
    expect(screen.getByText('Needs restart')).toBeInTheDocument();
    expect(screen.getByText('4 to 128')).toBeInTheDocument();
    expect(screen.getByText('At least 0')).toBeInTheDocument();
    expect(screen.getByText('At most 2')).toBeInTheDocument();
    expect(screen.queryByText('keys')).toBeNull();
    expect(screen.getByText('All changes saved')).toBeInTheDocument();

    // bool
    await fireEvent.click(screen.getByRole('switch', { name: 'Proximity chat' }));
    expect(modSettings.draft.proximity).toBe(false);
    expect(screen.getByText('1 unsaved change')).toBeInTheDocument();
    // int: a broken entry stays text and is named
    const radius = screen.getByRole('spinbutton', { name: 'Radius' });
    await fireEvent.input(radius, { target: { value: '10' } });
    expect(screen.getByText('Must go in steps of 4.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save settings' })).toBeDisabled();
    await fireEvent.input(radius, { target: { value: '' } });
    expect(modSettings.draft.radius).toBe('');
    expect(screen.getByText('Enter a number.')).toBeInTheDocument();
    await fireEvent.input(radius, { target: { value: '36' } });
    expect(modSettings.draft.radius).toBe(36);
    // float, enum, string
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Muffle' }), { target: { value: '0.5' } });
    expect(modSettings.draft.muffle).toBe(0.5);
    await fireEvent.change(screen.getByRole('combobox', { name: 'Curve' }), { target: { value: 'linear' } });
    expect(modSettings.draft.curve).toBe('linear');
    await fireEvent.input(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'toolong' } });
    expect(screen.getByText('At most 5 characters.')).toBeInTheDocument();
    await fireEvent.input(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'hi' } });
    // roles: built-ins, stored extra groups, a new group
    const who = screen.getByRole('group', { name: 'Who may mute' });
    expect(within(who).getByRole('button', { name: 'vip' })).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.click(within(who).getByRole('button', { name: 'Moderators' }));
    await fireEvent.click(within(who).getByRole('button', { name: 'vip' }));
    expect(modSettings.draft.admins).toEqual(['admin', 'mod']);
    const add = within(who).getByRole('textbox', { name: 'Another role or group for Who may mute' });
    expect(within(who).getByRole('button', { name: 'Add role' })).toBeDisabled();
    await fireEvent.input(add, { target: { value: ' builders ' } });
    await fireEvent.submit(add.closest('form')!);
    expect(modSettings.draft.admins).toEqual(['admin', 'mod', 'builders']);
    await fireEvent.input(add, { target: { value: 'admin' } });
    await fireEvent.submit(add.closest('form')!);
    expect(modSettings.draft.admins).toEqual(['admin', 'mod', 'builders']);
    // matrix of checkboxes
    const cell = screen.getByRole('checkbox', { name: 'global: Owner' });
    await fireEvent.click(cell);
    expect(modSettings.draft.talk).toEqual({ proximity: { everyone: true }, global: { owner: true } });
    await fireEvent.click(screen.getByRole('checkbox', { name: 'proximity: Everyone' }));
    expect(modSettings.draft.talk).toEqual({ global: { owner: true } });
    // matrix of role chips
    const rc = screen.getByRole('group', { name: 'global: talk' });
    expect(within(rc).queryByRole('textbox')).toBeNull();
    await fireEvent.click(within(rc).getByRole('button', { name: 'Everyone' }));
    expect(modSettings.draft.who).toEqual({ global: { talk: ['everyone'] } });
    // list: add up to max_items, edit an item, remove one
    expect(screen.getByText('Nothing added yet.')).toBeInTheDocument();
    expect(screen.getByText('0 of 2')).toBeInTheDocument();
    const addItem = screen.getByRole('button', { name: 'Add' });
    await fireEvent.click(addItem);
    await fireEvent.click(addItem);
    expect(addItem).toBeDisabled();
    expect(modSettings.draft.channels).toEqual([{ name: '', size: 1 }, { name: '', size: 1 }]);
    await fireEvent.input(screen.getByRole('textbox', { name: 'Name #2' }), { target: { value: 'much too long' } });
    expect(screen.getByText('At most 8 characters.')).toBeInTheDocument();
    await fireEvent.input(screen.getByRole('textbox', { name: 'Name #2' }), { target: { value: 'red' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Remove item 1' }));
    expect(modSettings.draft.channels).toEqual([{ name: 'red', size: 1 }]);

    // Reset one field, save the rest.
    await fireEvent.click(screen.getByRole('button', { name: 'Reset Curve to default' }));
    expect(modSettings.draft.curve).toBe('log');
    await fireEvent.click(screen.getByRole('button', { name: 'Save settings' }));
    await waitFor(() => expect(screen.getByText('All changes saved')).toBeInTheDocument());
    expect(mock.api.modSettingsSet).toHaveBeenCalledWith('voice', 'channels', [{ name: 'red', size: 1 }]);
    expect(mock.api.modSettingsSet).not.toHaveBeenCalledWith('voice', 'curve', expect.anything());
    await fireEvent.click(screen.getByRole('button', { name: 'Reset all to defaults' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Discard changes' }));
    expect(screen.getByText('All changes saved')).toBeInTheDocument();
    // Back to the overview: the edits stay, the panel does not load again.
    await fireEvent.click(screen.getByRole('tab', { name: 'Overview' }));
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    expect(mock.api.modSettingsGet).toHaveBeenCalledTimes(1);
  });

  it('backend refusals inline, restart notice, errors and empty schemas', async () => {
    const set = vi.fn(async (_id: string, key: string) => {
      if (key === 'talk') throw { code: 'setting', detail: 'unknown', got: 'global.x' };
      if (key === 'radius') throw { code: 'setting', detail: 'max', expected: '64' };
    });
    open(10, { modSettingsSet: set });
    store.state = 'running';
    mods.tab = 'settings';
    await screen.findByRole('heading', { name: 'Modes' });
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Radius' }), { target: { value: '100' } });
    await fireEvent.click(screen.getByRole('checkbox', { name: 'global: Owner' }));
    await fireEvent.click(screen.getByRole('switch', { name: 'Proximity chat' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save settings' }));
    expect(await screen.findByText('global.x: This entry is unknown to the mod.')).toBeInTheDocument();
    expect(screen.getByText('Must be at most 64.')).toBeInTheDocument();
    // A restart-marked field saved while running: nothing here, radius was refused.
    expect(screen.queryByText('Restart to apply some changes')).toBeNull();
    set.mockImplementation(async () => {});
    await fireEvent.click(screen.getByRole('button', { name: 'Save settings' }));
    expect(await screen.findByText('Restart to apply some changes')).toBeInTheDocument();
    const restart = vi.spyOn(store, 'restart').mockResolvedValue(undefined as never);
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    expect(restart).toHaveBeenCalled();
    restart.mockRestore();
    // A list over its limit shows the list-level message; item problems sit next to the item.
    modSettings.set('channels', [{}, {}, {}]);
    expect(await screen.findByText('At most 2 items.')).toBeInTheDocument();
    modSettings.set('channels', [{ name: 'much too long' }]);
    await waitFor(() => expect(screen.queryByText('At most 2 items.')).toBeNull());
    expect(screen.queryByText(/channels\[0\]/)).toBeNull();
  });

  it('load errors retry; mods without server settings', async () => {
    open(10, { modSettingsGet: vi.fn(async () => Promise.reject({ code: 'settingsSchema', detail: 'a: unknown type' })) });
    mods.tab = 'settings';
    expect(await screen.findByText('Couldn’t load the settings')).toBeInTheDocument();
    expect(screen.getByText('The mod’s settings are broken: a: unknown type')).toBeInTheDocument();
    let done!: (v: ModSettings) => void;
    mock.api.modSettingsGet = vi.fn(() => new Promise<ModSettings>((r) => (done = r)));
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(screen.getByText('Loading settings…')).toBeInTheDocument();
    done({ fields: [f({ key: 'k', type: 'bool', scope: 'client' })], values: {} });
    expect(await screen.findByText('No settings here')).toBeInTheDocument();
    cleanup();
    // No settings: no tabs, the overview even when the settings tab was last open.
    open(0);
    mods.tab = 'settings';
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
    mods.open('voice', 'settings');
    expect(mods.tab).toBe('settings');
    mods.open('voice');
    expect(mods.tab).toBe('overview');
  });

  it('controls follow values that are not set or of another shape', async () => {
    const changes: Json[] = [];
    const onchange = (v: Json) => changes.push(v);
    const [, int, , , en, str, roles, mtx, , list] = schema().fields;
    render(SettingControl, { field: int, value: null, label: 'Radius', onchange });
    expect(screen.getByRole('spinbutton', { name: 'Radius' })).toHaveValue(null);
    cleanup();
    render(SettingControl, { field: { ...int, type: 'float', step: undefined }, value: 1, label: 'F', onchange });
    expect(screen.getByRole('spinbutton', { name: 'F' })).toHaveAttribute('step', 'any');
    cleanup();
    render(SettingControl, { field: { ...en, values: undefined }, value: 'x', label: 'E', onchange });
    expect(screen.getByRole('combobox', { name: 'E' }).querySelectorAll('option')).toHaveLength(0);
    cleanup();
    render(SettingControl, { field: str, value: null, label: 'S', onchange });
    expect(screen.getByRole('textbox', { name: 'S' })).toHaveValue('');
    cleanup();
    render(SettingControl, { field: roles, value: 'admin', label: 'R', onchange });
    expect(screen.getByRole('button', { name: 'Admins' })).toHaveAttribute('aria-pressed', 'false');
    cleanup();
    render(SettingControl, { field: { ...mtx, rows: undefined, cols: undefined }, value: [], label: 'M', onchange });
    expect(screen.getByRole('table')).toBeInTheDocument();
    cleanup();
    render(SettingControl, { field: { ...mtx, cols: undefined }, value: {}, label: 'M', onchange });
    expect(screen.getAllByRole('row')).toHaveLength(3);
    cleanup();
    render(SettingControl, { field: mtx, value: { global: 3 }, label: 'M', onchange });
    expect(screen.getByRole('checkbox', { name: 'global: Owner' })).not.toBeChecked();
    cleanup();
    render(SettingControl, { field: { ...list, max_items: undefined }, value: [7, { size: 3, extra: 1 }], label: 'L', onchange });
    expect(screen.queryByText(/of/)).toBeNull();
    expect(screen.getByRole('spinbutton', { name: 'Size #2' })).toHaveValue(3);
    await fireEvent.input(screen.getByRole('textbox', { name: 'Name #1' }), { target: { value: 'a' } });
    expect(changes.at(-1)).toEqual([{ name: 'a' }, { size: 3, extra: 1 }]);
    cleanup();
    render(SettingControl, { field: list, value: 'nope', label: 'L', onchange });
    expect(screen.getByText('Nothing added yet.')).toBeInTheDocument();
    cleanup();
    const nested = f({ key: 'n', type: 'list', item: { r: { type: 'roles', default: [], label: { en: 'R' }, scope: 'server', restart: false } } });
    render(SettingControl, { field: nested, value: [{}], label: 'N', onchange });
    expect(screen.getByRole('group', { name: 'R #1' })).toBeInTheDocument();
  });
});
