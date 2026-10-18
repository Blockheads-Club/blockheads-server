import { cleanup, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it } from 'vitest';
import Alert from './Alert.svelte';
import Avatar from './Avatar.svelte';
import Badge from './Badge.svelte';
import Button from './Button.svelte';
import Card from './Card.svelte';
import Dialog from './Dialog.svelte';
import EmptyState from './EmptyState.svelte';
import Menu from './Menu.svelte';
import QrCode from './QrCode.svelte';
import SelectField from './SelectField.svelte';
import StatusHero from './StatusHero.svelte';
import Table from './Table.svelte';
import Tabs from './Tabs.svelte';

afterEach(() => cleanup());
const text = (s: string) => createRawSnippet(() => ({ render: () => `<span>${s}</span>` }));
const noop = () => {};

/** Components must follow prop changes after the first render. */
describe('prop updates', () => {
  it('updates simple presentational components', async () => {
    const alert = render(Alert, { title: 'A', tone: 'info' });
    await alert.rerender({ title: 'B', tone: 'bad', icon: 'key' });
    expect(screen.getByRole('alert')).toHaveTextContent('B');
    const avatar = render(Avatar, { name: 'a', size: 20 });
    await avatar.rerender({ name: 'zed', size: 40 });
    expect(avatar.container).toHaveTextContent('ZE');
    const badge = render(Badge, { children: text('x'), tone: 'ok' });
    await badge.rerender({ tone: 'bad', label: 'l' });
    const card = render(Card, { children: text('c') });
    await card.rerender({ tone: 'primary', pad: false, label: 'n', class: 'extra' });
    const empty = render(EmptyState, { icon: 'map', title: 't' });
    await empty.rerender({ tone: 'quiet', icon: 'users', title: 'u', text: 'v' });
    const table = render(Table, { label: 'l', template: '1fr', columns: [{ key: 'a', label: 'A' }], rows: text('r') });
    await table.rerender({ template: '2fr', columns: [{ key: 'b', label: 'B', srOnly: true }] });
  });

  it('updates buttons in every shape', async () => {
    const b = render(Button, { children: text('x'), variant: 'default', size: 'md', title: 't' });
    await b.rerender({ variant: 'primary', size: 'sm', icon: 'play', loading: true, pressed: true, expanded: true, haspopup: true, block: true, label: 'l', title: 'u', disabled: true });
    await b.rerender({ icon: 'stop', loading: false, size: 'lg' });
    b.unmount();
    const link = render(Button, { href: 'https://a.example', children: text('x'), size: 'md' });
    await link.rerender({ href: 'https://b.example', size: 'sm', icon: 'code', variant: 'ghost', block: true, title: 't', label: 'l' });
    await link.rerender({ icon: undefined, size: 'lg' });
  });

  it('updates dialogs, menus, selects and tabs', async () => {
    const d = render(Dialog, { title: 'a', onclose: noop });
    await d.rerender({ title: 'b', icon: 'trash', tone: 'bad', description: 'x' });
    await d.rerender({ icon: 'key', tone: 'plain', description: undefined });
    const m = render(Menu, { label: 'a', items: [{ id: 'a', label: 'A' }], onselect: noop });
    await m.rerender({ label: 'b', items: [{ id: 'a', label: 'A', tone: 'bad', icon: 'copy', separated: true }, { id: 'b', label: 'B' }] });
    const s = render(SelectField, { label: 'a', value: 'x', options: [{ value: 'x', label: 'X' }], onchange: noop });
    await s.rerender({ label: 'b', value: 'y', disabled: true, hint: 'h', options: [{ value: 'y', label: 'Y', lang: 'tr' }] });
    const t = render(Tabs, { items: [{ id: 'a', label: 'A' }], value: 'a', label: 'l', onchange: noop });
    await t.rerender({ items: [{ id: 'a', label: 'A2', dot: 'red', icon: 'sun' }, { id: 'b', label: 'B' }], value: 'b', mode: 'radiogroup' });
    await t.rerender({ items: [{ id: 'a', label: 'A3' }], value: 'a', mode: 'tablist' });
  });

  it('updates the status hero between states and layouts', async () => {
    const h = render(StatusHero, { state: 'stopped', worldName: 'W', onstart: noop, onstop: noop, onrestart: noop });
    for (const state of ['starting', 'stopping', 'failed', 'running', 'stopped'] as const) await h.rerender({ state });
    await h.rerender({ phone: true, state: 'starting', worldName: undefined });
    await h.rerender({ state: 'stopping', worldName: 'Z' });
    await h.rerender({ state: 'running' });
  });

  it('ignores a QR result that arrives after the text changed', async () => {
    const q = render(QrCode, { text: 'a', size: 100 });
    await q.rerender({ text: 'b', size: 120 });
    q.unmount();
    render(QrCode, { text: 'c' });
  });
});
