import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { setLang } from '../i18n.svelte';
import type { LogEntry } from '../types';
import ConsoleView from './ConsoleView.svelte';

afterEach(() => cleanup());
setLang('en');

const E = (id: number, level: LogEntry['level'], text: string): LogEntry => ({ id, at: 1_700_000_000_000 + id, level, text });
const entries = [E(1, 'info', 'World load complete.'), E(2, 'chat', 'MIRA: hi mira'), E(3, 'join', 'x - Player Connected MIRA | 1 | 2'), E(4, 'error', 'Save failed')];
const base = { entries, active: true, history: ['/players', '/help'], onsend: () => {} };

describe('ConsoleView', () => {
  it('renders level tags and an empty state', () => {
    render(ConsoleView, base);
    expect(screen.getByText('CHAT')).toBeInTheDocument();
    cleanup();
    render(ConsoleView, { ...base, entries: [] });
    expect(screen.getByText('Nothing logged yet.')).toBeInTheDocument();
  });

  it('filters by level and clears the filter with All', async () => {
    render(ConsoleView, base);
    await fireEvent.click(screen.getByRole('button', { name: 'Errors' }));
    expect(screen.queryByText('CHAT')).toBeNull();
    expect(screen.getByText('ERROR')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Chat' }));
    expect(screen.getByText('CHAT')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Chat' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Errors' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Warnings' }));
    expect(screen.getByText('No lines match the current filters.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'All' }));
    expect(screen.getByText('INFO')).toBeInTheDocument();
  });

  it('searches, highlights and steps through matches', async () => {
    render(ConsoleView, base);
    const q = screen.getByLabelText('Search log');
    await fireEvent.input(q, { target: { value: 'mira' } });
    expect(document.querySelectorAll('mark')).toHaveLength(3);
    expect(screen.getByText('1 of 2')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Next match' }));
    expect(screen.getByText('2 of 2')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Previous match' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Previous match' }));
    expect(screen.getByText('2 of 2')).toBeInTheDocument();
    await fireEvent.input(q, { target: { value: 'zzz' } });
    expect(screen.getByText('No match')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Next match' }).closest('button') ?? document.body);
    await fireEvent.input(q, { target: { value: 'World load complete.' } });
    expect(screen.getByText('1 of 1')).toBeInTheDocument();
    await fireEvent.input(q, { target: { value: '' } });
  });

  it('focuses the search with Ctrl+F only', async () => {
    render(ConsoleView, base);
    await fireEvent.keyDown(window, { key: 'f' });
    expect(screen.getByLabelText('Search log')).not.toHaveFocus();
    await fireEvent.keyDown(window, { key: 'F', ctrlKey: true });
    expect(screen.getByLabelText('Search log')).toHaveFocus();
  });

  it('runs commands and ignores empty input', async () => {
    const onsend = vi.fn();
    render(ConsoleView, { ...base, onsend });
    const cmd = screen.getByLabelText('Server command');
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    await fireEvent.submit(cmd.closest('form')!);
    expect(onsend).not.toHaveBeenCalled();
    await fireEvent.input(cmd, { target: { value: ' /players ' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    expect(onsend).toHaveBeenCalledWith('/players');
    expect(cmd).toHaveValue('');
  });

  it('walks the command history with the arrow keys and restores the draft', async () => {
    render(ConsoleView, base);
    const cmd = screen.getByLabelText('Server command');
    await fireEvent.input(cmd, { target: { value: '/dra' } });
    await fireEvent.keyDown(cmd, { key: 'ArrowDown' });
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    expect(cmd).toHaveValue('/players');
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    expect(cmd).toHaveValue('/help');
    await fireEvent.keyDown(cmd, { key: 'ArrowDown' });
    expect(cmd).toHaveValue('/players');
    await fireEvent.keyDown(cmd, { key: 'ArrowDown' });
    expect(cmd).toHaveValue('/dra');
    expect(screen.queryByRole('listbox')).toBeNull();
    await fireEvent.keyDown(cmd, { key: 'Escape' });
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    await fireEvent.keyDown(cmd, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).toBeNull();
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    await fireEvent.click(screen.getByRole('button', { name: '/help' }));
    expect(cmd).toHaveValue('/help');
  });

  it('does nothing on ArrowUp without history', async () => {
    render(ConsoleView, { ...base, history: [] });
    const cmd = screen.getByLabelText('Server command');
    await fireEvent.keyDown(cmd, { key: 'ArrowUp' });
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('completes commands with Tab', async () => {
    render(ConsoleView, base);
    const cmd = screen.getByLabelText('Server command');
    await fireEvent.input(cmd, { target: { value: '/pl' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    expect(cmd).toHaveValue('/players ');
    await fireEvent.input(cmd, { target: { value: '/un' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    expect(cmd).toHaveValue('/un');
    await fireEvent.input(cmd, { target: { value: '/u' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    expect(cmd).toHaveValue('/un');
    await fireEvent.input(cmd, { target: { value: '/zzz' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    expect(cmd).toHaveValue('/zzz');
    await fireEvent.input(cmd, { target: { value: 'plain' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    await fireEvent.input(cmd, { target: { value: '/kick ' } });
    await fireEvent.keyDown(cmd, { key: 'Tab' });
    expect(cmd).toHaveValue('/kick ');
  });

  it('disables the command line while the server is stopped', () => {
    render(ConsoleView, { ...base, active: false });
    expect(screen.getByLabelText('Server command')).toBeDisabled();
    expect(screen.getByPlaceholderText('Start the server to send commands')).toBeInTheDocument();
  });

  it('offers quick commands on phones', async () => {
    render(ConsoleView, { ...base, phone: true });
    await fireEvent.click(screen.getByRole('button', { name: '/kick' }));
    expect(screen.getByLabelText('Server command')).toHaveValue('/kick ');
    expect(screen.getByRole('button', { name: 'Run' })).toBeInTheDocument();
  });

  it('follows new lines and offers a jump button after scrolling away', async () => {
    const r = render(ConsoleView, base);
    await new Promise((r) => setTimeout(r));
    const log = screen.getByTestId('log');
    let height = 1000;
    Object.defineProperty(log, 'scrollHeight', { configurable: true, get: () => height });
    Object.defineProperty(log, 'clientHeight', { configurable: true, value: 200 });
    log.scrollTop = 0;
    await fireEvent.scroll(log);
    await r.rerender({ entries: [...entries, E(5, 'info', 'later'), E(6, 'info', 'later 2')] });
    expect(await screen.findByRole('button', { name: '2 new lines' })).toBeInTheDocument();
    await fireEvent.scroll(log);
    await fireEvent.click(screen.getByRole('button', { name: '2 new lines' }));
    expect(screen.queryByRole('button', { name: /new line/ })).toBeNull();
    log.scrollTop = 900;
    await fireEvent.scroll(log);
    height = 1100;
    await r.rerender({ entries: [...entries, E(7, 'info', 'x')], paused: true });
  });
});
