import { beforeEach, describe, expect, it } from 'vitest';
import { en, tr } from './messages';
import { errText, i18n, setLang, t } from './i18n.svelte';
import { ago, duration } from './format';

describe('i18n', () => {
  beforeEach(() => setLang('en'));

  it('defaults to English and interpolates', () => {
    expect(t('nav.worlds')).toBe('Worlds');
    expect(t('worlds.deleteTitle', { name: 'A' })).toContain('A');
    expect(t('side.port')).toBe('port {port}');
  });

  it('switches to Turkish and falls back for unknown languages', () => {
    setLang('tr');
    expect(t('nav.settings')).toBe('Ayarlar');
    expect(document.documentElement.lang).toBe('tr');
    setLang('xx');
    expect(i18n.lang).toBe('en');
  });

  it('falls back to English or the key when a language lacks a message', () => {
    i18n.lang = 'zz';
    expect(t('nav.worlds')).toBe('Worlds');
    expect(t('nope' as never)).toBe('nope');
  });

  it('resolves plural forms per language', () => {
    expect(t('console.lines', { n: 1 })).toBe('1 line');
    expect(t('console.lines', { n: 3 })).toBe('3 lines');
    setLang('tr');
    expect(t('console.lines', { n: 3 })).toBe('3 satır');
  });

  it('falls back to the other form when a plural rule has no case', () => {
    i18n.lang = 'ar';
    expect(t('console.lines', { n: 2 })).toBe('2 lines');
    setLang('en');
  });

  it('has the same keys in every language', () => {
    expect(Object.keys(tr).sort()).toEqual(Object.keys(en).sort());
    for (const v of Object.values(tr)) expect(v.length).toBeGreaterThan(0);
  });

  it('maps backend error codes', () => {
    expect(errText('running')).toBe(en['err.running']);
    expect(errText('something odd')).toBe('something odd');
  });

  it('formats durations and relative times', () => {
    expect(duration(134 * 60000)).toBe('2h 14m');
    expect(duration(5 * 60000)).toBe('5 min');
    expect(duration(1000)).toBe('under 1 min');
    expect(ago(1000)).toBe('just now');
    expect(ago(6 * 60000)).toBe('6 min ago');
    setLang('tr');
    expect(duration(134 * 60000)).toBe('2 sa 14 dk');
  });
});
