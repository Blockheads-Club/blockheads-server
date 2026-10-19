import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setLang } from '../i18n.svelte';
import { viewport } from '../viewport.svelte';
import Alert from './Alert.svelte';
import Avatar from './Avatar.svelte';
import Badge from './Badge.svelte';
import Button from './Button.svelte';
import Card from './Card.svelte';
import ChipInput from './ChipInput.svelte';
import Dialog from './Dialog.svelte';
import EmptyState from './EmptyState.svelte';
import Field from './Field.svelte';
import Icon from './Icon.svelte';
import Menu from './Menu.svelte';
import PerfCard from './PerfCard.svelte';
import QrCard from './QrCard.svelte';
import SelectField from './SelectField.svelte';
import SettingRow from './SettingRow.svelte';
import StatusBanners from './StatusBanners.svelte';
import StatusHero from './StatusHero.svelte';
import StatusPill from './StatusPill.svelte';
import Table from './Table.svelte';
import Tabs from './Tabs.svelte';
import Toasts from './Toasts.svelte';
import Toggle from './Toggle.svelte';
import { dismiss, toast, toasts } from './toast.svelte';

const text = (s: string) => createRawSnippet(() => ({ render: () => `<span>${s}</span>` }));

beforeEach(() => {
  setLang('en');
  viewport.phone = false;
  toasts.items = [];
});
afterEach(() => cleanup());

describe('Button', () => {
  it('renders variants and handles clicks', async () => {
    const onclick = vi.fn();
    render(Button, { children: text('Go'), icon: 'play', size: 'sm', variant: 'primary', onclick, pressed: true, expanded: false, haspopup: true });
    const b = screen.getByRole('button', { name: 'Go' });
    expect(b).toHaveAttribute('aria-pressed', 'true');
    expect(b).toHaveAttribute('aria-haspopup', 'menu');
    await fireEvent.click(b);
    expect(onclick).toHaveBeenCalled();
  });

  it('shows a spinner while loading and disables itself', () => {
    render(Button, { children: text('Starting…'), loading: true });
    const b = screen.getByRole('button');
    expect(b).toBeDisabled();
    expect(b).toHaveAttribute('aria-busy', 'true');
  });

  it('is icon-only without children and renders links', () => {
    render(Button, { icon: 'x', label: 'Close' });
    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument();
    cleanup();
    render(Button, { href: 'https://example.org', children: text('Site'), icon: 'code', size: 'sm' });
    expect(screen.getByRole('link', { name: 'Site' })).toHaveAttribute('rel', 'noopener noreferrer');
    cleanup();
    render(Button, { href: 'https://example.org', icon: 'code', label: 'Repo' });
    expect(screen.getByRole('link', { name: 'Repo' })).toBeInTheDocument();
    cleanup();
    render(Button, { href: 'https://example.org', children: text('Mid'), icon: 'code' });
    cleanup();
    render(Button, { href: 'https://example.org', children: text('Plain') });
    expect(screen.getByRole('link')).toBeInTheDocument();
    cleanup();
    render(Button, { children: text('Plain') });
    expect(screen.getByRole('button')).toBeInTheDocument();
  });
});

describe('Badge, StatusPill, Card, Alert, EmptyState, Avatar, Icon', () => {
  it('renders badge parts', () => {
    render(Badge, { children: text('Hi'), dot: true, pulse: true, icon: 'check', label: 'greeting' });
    expect(screen.getByLabelText('greeting')).toBeInTheDocument();
    cleanup();
    render(Badge, { children: text('Plain') });
    expect(screen.getByText('Plain')).toBeInTheDocument();
  });

  it('names the server status for screen readers', () => {
    for (const state of ['running', 'starting', 'stopping', 'stopped', 'failed'] as const) {
      render(StatusPill, { state });
      expect(screen.getByRole('status')).toHaveAttribute('aria-label', expect.stringContaining('Server status'));
      cleanup();
    }
  });

  it('renders a card with optional tone and label', () => {
    render(Card, { children: text('in'), tone: 'ok', pad: false, label: 'box' });
    expect(screen.getByLabelText('box')).toBeInTheDocument();
    cleanup();
    render(Card, { children: text('in') });
    expect(screen.getByText('in')).toBeInTheDocument();
  });

  it('alerts announce errors and accept actions', () => {
    render(Alert, { tone: 'bad', title: 'Broke', children: text('details'), actions: text('act') });
    expect(screen.getByRole('alert')).toHaveTextContent('Broke');
    expect(screen.getByText('act')).toBeInTheDocument();
    cleanup();
    render(Alert, { title: 'Note', icon: 'key' });
    expect(screen.getByRole('note')).toBeInTheDocument();
  });

  it('shows empty states', () => {
    render(EmptyState, { icon: 'map', title: 'Nothing', text: 'Add one', children: text('btn') });
    expect(screen.getByRole('heading', { name: 'Nothing' })).toBeInTheDocument();
    cleanup();
    render(EmptyState, { icon: 'map', title: 'Bare', tone: 'quiet' });
    expect(screen.getByText('Bare')).toBeInTheDocument();
  });

  it('renders avatars and icons', () => {
    const { container } = render(Avatar, { name: 'mira' });
    expect(container).toHaveTextContent('MI');
    cleanup();
    const r = render(Icon, { name: 'check' });
    expect(r.container.querySelector('svg')).toBeInTheDocument();
  });
});

describe('Tabs', () => {
  it('switches with the mouse and arrow keys in both modes', async () => {
    const onchange = vi.fn();
    const items = [{ id: 'a', label: 'A', dot: 'red' }, { id: 'b', label: 'B', icon: 'sun' as const }, { id: 'c', label: 'C' }];
    render(Tabs, { items, value: 'a', label: 'Letters', onchange });
    const a = screen.getByRole('tab', { name: 'A' });
    expect(a).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(a, { key: 'ArrowRight' });
    expect(onchange).toHaveBeenLastCalledWith('b');
    await fireEvent.keyDown(a, { key: 'ArrowLeft' });
    expect(onchange).toHaveBeenLastCalledWith('c');
    await fireEvent.keyDown(a, { key: 'x' });
    expect(onchange).toHaveBeenCalledTimes(2);
    await fireEvent.click(screen.getByRole('tab', { name: 'B' }));
    expect(onchange).toHaveBeenLastCalledWith('b');
    cleanup();
    render(Tabs, { items, value: 'b', label: 'Letters', mode: 'radiogroup', onchange });
    expect(screen.getByRole('radio', { name: 'B' })).toHaveAttribute('aria-checked', 'true');
  });
});

describe('Toggle, Field, SelectField, SettingRow', () => {
  it('toggles and reports', async () => {
    const onchange = vi.fn();
    render(Toggle, { label: 'Whitelist', onchange });
    await fireEvent.click(screen.getByRole('switch', { name: 'Whitelist' }));
    expect(onchange).toHaveBeenCalledWith(true);
    cleanup();
    render(Toggle, { label: 'Hidden', hideLabel: true, disabled: true });
    expect(screen.getByRole('switch')).toBeDisabled();
    cleanup();
    render(Toggle, { label: 'No callback' });
    await fireEvent.click(screen.getByRole('switch'));
  });

  it('wires labels, hints and errors', async () => {
    const oninput = vi.fn();
    render(Field, { label: 'Port', hint: 'help', oninput, icon: 'search', mono: true, center: true, width: '100px' });
    const f = screen.getByLabelText('Port');
    expect(f).toHaveAccessibleDescription('help');
    await fireEvent.input(f, { target: { value: '12' } });
    expect(oninput).toHaveBeenCalledWith('12');
    cleanup();
    render(Field, { label: 'Port', error: 'bad', hideLabel: true, trailing: text('x') });
    expect(screen.getByLabelText('Port')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByText('bad')).toBeInTheDocument();
    cleanup();
    render(Field, { label: 'Plain' });
    await fireEvent.input(screen.getByLabelText('Plain'), { target: { value: 'z' } });
  });

  it('selects an option', async () => {
    const onchange = vi.fn();
    render(SelectField, { label: 'Lang', value: 'en', options: [{ value: 'en', label: 'English', lang: 'en' }, { value: 'tr', label: 'Türkçe' }], onchange, hint: 'h' });
    await fireEvent.change(screen.getByLabelText('Lang'), { target: { value: 'tr' } });
    expect(onchange).toHaveBeenCalledWith('tr');
    cleanup();
    render(SelectField, { label: 'Hidden', hideLabel: true, disabled: true, value: 'a', options: [{ value: 'a', label: 'A' }], onchange });
    expect(screen.getByLabelText('Hidden')).toBeDisabled();
  });

  it('renders setting rows', () => {
    render(SettingRow, { title: 'Port', hint: '15151', restart: true, mono: true, titleId: 'x', children: text('ctl') });
    expect(screen.getByText('Restart')).toBeInTheDocument();
    cleanup();
    render(SettingRow, { title: 'Bare' });
    expect(screen.getByText('Bare')).toBeInTheDocument();
  });
});

describe('Dialog', () => {
  it('traps focus, closes on Escape, scrim and button, and restores focus', async () => {
    const opener = document.createElement('button');
    document.body.append(opener);
    opener.focus();
    const onclose = vi.fn();
    const { unmount } = render(Dialog, { title: 'Sure?', onclose, icon: 'trash', tone: 'bad', description: 'Careful', children: text('body'), footer: createRawSnippet(() => ({ render: () => '<button data-autofocus>OK</button>' })) });
    const dlg = screen.getByRole('dialog', { name: 'Sure?' });
    await waitFor(() => expect(screen.getByText('OK')).toHaveFocus());
    const close = screen.getByRole('button', { name: 'Close' });
    // Tab wraps from the last to the first control and Shift+Tab back.
    screen.getByText('OK').focus();
    await fireEvent.keyDown(dlg, { key: 'Tab' });
    expect(close).toHaveFocus();
    await fireEvent.keyDown(dlg, { key: 'Tab', shiftKey: true });
    expect(screen.getByText('OK')).toHaveFocus();
    await fireEvent.keyDown(dlg, { key: 'Tab' });
    close.focus();
    await fireEvent.keyDown(dlg, { key: 'Tab' });
    await fireEvent.keyDown(dlg, { key: 'a' });
    await fireEvent.keyDown(dlg, { key: 'Escape' });
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.mouseDown(dlg);
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.mouseDown(dlg.parentElement!);
    expect(onclose).toHaveBeenCalledTimes(2);
    await fireEvent.click(close);
    expect(onclose).toHaveBeenCalledTimes(3);
    unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });

  it('works without optional parts', async () => {
    render(Dialog, { title: 'Plain', onclose: () => {} });
    const dlg = screen.getByRole('dialog');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus());
    await fireEvent.keyDown(dlg, { key: 'Tab' });
  });
});

describe('Menu', () => {
  const items = [{ id: 'a', label: 'Alpha', icon: 'copy' as const }, { id: 'b', label: 'Beta', tone: 'bad' as const, separated: true }];

  it('opens, navigates with the keyboard, selects and closes', async () => {
    const onselect = vi.fn();
    render(Menu, { label: 'Actions', items, onselect });
    const trigger = screen.getByRole('button', { name: 'Actions' });
    await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    await fireEvent.click(trigger);
    const alpha = screen.getByRole('menuitem', { name: 'Alpha' });
    await waitFor(() => expect(alpha).toHaveFocus());
    await fireEvent.keyDown(alpha, { key: 'ArrowDown' });
    expect(screen.getByRole('menuitem', { name: 'Beta' })).toHaveFocus();
    await fireEvent.keyDown(alpha, { key: 'ArrowUp' });
    await fireEvent.keyDown(alpha, { key: 'x' });
    await fireEvent.keyDown(alpha, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(trigger).toHaveFocus();
    await fireEvent.click(trigger);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Beta' }));
    expect(onselect).toHaveBeenCalledWith('b');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('closes on outside press and Tab, and ignores keys while closed', async () => {
    render(Menu, { label: 'Actions', items, onselect: () => {} });
    const trigger = screen.getByRole('button', { name: 'Actions' });
    await fireEvent.mouseDown(document.body);
    await fireEvent.click(trigger);
    await fireEvent.mouseDown(screen.getByRole('menu'));
    expect(screen.getByRole('menu')).toBeInTheDocument();
    await fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('menu')).toBeNull();
    await fireEvent.click(trigger);
    await fireEvent.keyDown(screen.getByRole('menuitem', { name: 'Alpha' }), { key: 'Tab' });
    expect(screen.queryByRole('menu')).toBeNull();
    await fireEvent.click(trigger);
    await fireEvent.click(trigger);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('becomes a bottom sheet on phones', async () => {
    viewport.phone = true;
    render(Menu, { label: 'Actions', items, onselect: () => {} });
    await fireEvent.click(screen.getByRole('button', { name: 'Actions' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('menu')).toBeNull();
  });
});

describe('Toasts', () => {
  it('shows, runs an action and dismisses', async () => {
    vi.useFakeTimers();
    const undo = vi.fn();
    render(Toasts);
    toast('Saved');
    const id = toast('Added', { label: 'Undo', run: undo }, 100);
    await vi.advanceTimersByTimeAsync(10);
    expect(screen.getByText('Saved')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(undo).toHaveBeenCalled();
    expect(screen.queryByText('Added')).toBeNull();
    dismiss(id);
    await vi.advanceTimersByTimeAsync(7000);
    expect(toasts.items).toHaveLength(0);
    vi.useRealTimers();
  });
});

describe('ChipInput', () => {
  const base = { label: 'Add admin', placeholder: 'Name', addLabel: 'Add', removeLabel: (n: string) => `Remove ${n}`, emptyText: 'Empty' };

  it('adds trimmed names and ignores blanks', async () => {
    const onadd = vi.fn();
    render(ChipInput, { ...base, entries: [], onadd, onremove: () => {} });
    expect(screen.getByText('Empty')).toBeInTheDocument();
    const input = screen.getByLabelText('Add admin');
    await fireEvent.input(input, { target: { value: '  Bob ' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(onadd).toHaveBeenCalledWith('Bob');
    await fireEvent.submit(input.closest('form')!);
    expect(onadd).toHaveBeenCalledTimes(1);
  });

  it('removes entries, with text or icon buttons, and expands long lists', async () => {
    const onremove = vi.fn();
    const entries = ['a', 'b', 'c'];
    render(ChipInput, { ...base, entries, limit: 2, onadd: () => {}, onremove });
    await fireEvent.click(screen.getByRole('button', { name: 'Remove a' }));
    expect(onremove).toHaveBeenCalledWith('a');
    expect(screen.queryByText('c')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Show 1 more' }));
    expect(screen.getByText('c')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Show less' }));
    cleanup();
    render(ChipInput, { ...base, entries: ['z'], removeText: 'Unban', disabled: true, onadd: () => {}, onremove });
    expect(screen.getByRole('button', { name: 'Remove z' })).toBeDisabled();
    expect(screen.getByText('Unban')).toBeInTheDocument();
  });
});

describe('Table', () => {
  it('renders header cells, hiding icon column names visually', () => {
    render(Table, {
      label: 'Things',
      template: '1fr 1fr',
      columns: [{ key: 'i', label: 'Icon', srOnly: true }, { key: 'n', label: 'Name', narrow: true, align: 'end' }],
      rows: createRawSnippet(() => ({ render: () => '<div class="tr" role="row"><span role="cell">x</span></div>' }))
    });
    expect(screen.getByRole('table', { name: 'Things' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: 'Name' })).toBeInTheDocument();
  });
});

describe('QrCard', () => {
  const addrs = [{ ip: '10.0.0.2', endpoint: '10.0.0.2:15151' }, { ip: '10.0.1.2', endpoint: '10.0.1.2:15151' }];

  it('explains that the server must run first', () => {
    render(QrCard, { addrs, port: 15151, running: false });
    expect(screen.getByText('Start the server to let players join')).toBeInTheDocument();
  });

  it('reports a missing network', () => {
    render(QrCard, { addrs: [], port: 15151, running: true });
    expect(screen.getByTestId('no-net')).toBeInTheDocument();
  });

  it('prints and copies the address, and switches networks', async () => {
    const writeText = vi.fn(async () => {});
    Object.assign(navigator, { clipboard: { writeText } });
    render(Toasts);
    render(QrCard, { addrs, port: 15151, running: true });
    expect(screen.getByTestId('addr')).toHaveTextContent('10.0.0.2');
    await waitFor(() => expect(screen.getByTestId('qr').querySelector('svg')).not.toBeNull());
    await fireEvent.click(screen.getByRole('radio', { name: '10.0.1.2' }));
    expect(screen.getByTestId('addr')).toHaveTextContent('10.0.1.2');
    await fireEvent.click(screen.getByRole('button', { name: 'Copy address' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('10.0.1.2:15151'));
    expect(await screen.findByText('Address copied')).toBeInTheDocument();
    writeText.mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(await screen.findByText(/Couldn’t copy/)).toBeInTheDocument();
  });

  it('renders the compact phone variant', () => {
    render(QrCard, { addrs: [addrs[0]], port: 1, running: true, phone: true });
    expect(screen.queryByRole('radiogroup')).toBeNull();
  });
});

describe('StatusHero and StatusBanners', () => {
  const handlers = { onstart: vi.fn(), onstop: vi.fn(), onrestart: vi.fn() };

  it('offers the right action per state', async () => {
    render(StatusHero, { state: 'stopped', worldName: 'W', ...handlers });
    await fireEvent.click(screen.getByRole('button', { name: 'Start server' }));
    expect(handlers.onstart).toHaveBeenCalled();
    cleanup();
    render(StatusHero, { state: 'failed', worldName: undefined, ...handlers });
    expect(screen.getByRole('button', { name: 'Try to start again' })).toBeDisabled();
    cleanup();
    render(StatusHero, { state: 'running', worldName: 'W', ...handlers });
    await fireEvent.click(screen.getByRole('button', { name: 'Restart' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Stop server' }));
    expect(handlers.onrestart).toHaveBeenCalled();
    expect(handlers.onstop).toHaveBeenCalled();
    cleanup();
    render(StatusHero, { state: 'starting', worldName: 'W', ...handlers });
    expect(screen.getByRole('button', { name: 'Starting…' })).toBeDisabled();
    cleanup();
    render(StatusHero, { state: 'stopping', worldName: 'W', ...handlers });
    expect(screen.getByRole('button', { name: 'Stopping…' })).toBeDisabled();
  });

  it('shows the world card on phones', () => {
    render(StatusHero, { state: 'running', worldName: 'Sunny', phone: true, ...handlers });
    expect(screen.getByText('Sunny')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Restart' })).toBeNull();
    cleanup();
    render(StatusHero, { state: 'stopped', worldName: undefined, phone: true, ...handlers });
    expect(screen.getByText('No world selected')).toBeInTheDocument();
  });

  it('shows banners for offline, failed and starting', async () => {
    const h = { onretry: vi.fn(), onport: vi.fn(), onlog: vi.fn() };
    render(StatusBanners, { state: 'failed', worldName: 'W', detail: 'port in use', offline: true, ...h });
    await fireEvent.click(screen.getByRole('button', { name: 'Retry now' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Change port' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Open log' }));
    expect(screen.getByText('port in use')).toBeInTheDocument();
    cleanup();
    render(StatusBanners, { state: 'failed', worldName: undefined, detail: null, offline: false, ...h });
    expect(screen.getByText(/Something went wrong/)).toBeInTheDocument();
    cleanup();
    render(StatusBanners, { state: 'starting', worldName: undefined, detail: null, offline: false, ...h });
    expect(screen.getAllByRole('status').length).toBeGreaterThan(0);
    cleanup();
    render(StatusBanners, { state: 'running', worldName: 'W', detail: null, offline: false, ...h });
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('PerfCard', () => {
  const at = (cpu: number, mem: number, memTotal = 16 * 2 ** 30) => ({ cpu, cores: 8, mem, memTotal });
  it('waits for the first sample, then shows CPU and memory with their last minute', () => {
    const r = render(PerfCard, { stats: null, history: [] });
    expect(screen.getByText('Measuring…')).toBeInTheDocument();
    r.unmount();
    const history = [at(5, 100 * 2 ** 20), at(40, 300 * 2 ** 20), at(12.6, 2.5 * 2 ** 30)];
    render(PerfCard, { stats: history[2], history });
    expect(screen.getByTestId('perf-cpu')).toHaveTextContent('13%');
    expect(screen.getByText('of 8 cores')).toBeInTheDocument();
    expect(screen.getByTestId('perf-mem')).toHaveTextContent('2.5 GB');
    expect(screen.getByText('of 16.0 GB')).toBeInTheDocument();
    expect(document.querySelectorAll('.spark polyline')[0].getAttribute('points')).toMatch(/^0\.0,/);
  });
  it('copes with one sample and an unknown total', () => {
    render(PerfCard, { stats: at(0, 50 * 2 ** 20, 0), history: [at(0, 50 * 2 ** 20, 0)] });
    expect(screen.getByTestId('perf-mem')).toHaveTextContent('50 MB');
    expect(document.querySelector('.spark polyline')?.getAttribute('points')).toBe('');
  });
});
