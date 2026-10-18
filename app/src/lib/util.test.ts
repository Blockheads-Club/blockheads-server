import { describe, expect, it, vi } from 'vitest';
import { avatarColor, classify, clock, copyText, initials, parts, shortId, storage } from './util';

describe('util', () => {
  it('classifies log lines', () => {
    expect(classify('W - Player Connected a | 1.1.1.1 | id')).toBe('join');
    expect(classify('W - Player Disconnected a')).toBe('join');
    expect(classify('Save failed')).toBe('error');
    expect(classify('Warning: slow')).toBe('warn');
    expect(classify('MIRA_K: hello')).toBe('chat');
    expect(classify('World load complete.')).toBe('info');
  });

  it('formats clock and parts', () => {
    expect(clock(new Date(2026, 0, 1, 9, 5, 7).getTime())).toBe('09:05:07');
    expect(parts(3_725_000)).toEqual({ h: 1, m: 2, s: 5 });
    expect(parts(-5)).toEqual({ h: 0, m: 0, s: 0 });
  });

  it('builds avatars', () => {
    expect(initials('mira_k')).toBe('MI');
    expect(initials('__')).toBe('?');
    expect(avatarColor('a')).toMatch(/^#/);
    expect(avatarColor('a')).toBe(avatarColor('a'));
  });

  it('shortens ids', () => {
    expect(shortId('abc')).toBe('abc');
    expect(shortId('0123456789abcdef')).toBe('0123…cdef');
  });

  it('copies text and reports refusal', async () => {
    const writeText = vi.fn(async () => {});
    Object.assign(navigator, { clipboard: { writeText } });
    expect(await copyText('x')).toBe(true);
    writeText.mockRejectedValueOnce(new Error('no'));
    expect(await copyText('x')).toBe(false);
  });

  it('survives missing storage', () => {
    expect(storage((s) => s.length, -1)).toBeGreaterThanOrEqual(0);
    expect(storage(() => { throw new Error('blocked'); }, 7)).toBe(7);
  });
});
