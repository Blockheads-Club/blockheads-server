import { beforeEach, describe, expect, it, vi } from 'vitest';
import { en, tr } from './messages';
import { ru } from './messages.ru';
import { kk } from './messages.kk';
import { LANGUAGES, detectLang, errText, i18n, setLang, t } from './i18n.svelte';
import { ago, bytes, duration, percent } from './format';

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

  it('has the same keys and placeholders in every language', () => {
    const holes = (s: string) => [...s.replace(/\{\w+, plural,(?:\s*\w+ \{[^{}]*\})+\s*\}/g, (m) => `{${m.slice(1).split(',')[0]}}`).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const d of [tr, ru, kk]) {
      expect(Object.keys(d).sort()).toEqual(Object.keys(en).sort());
      for (const [k, v] of Object.entries(d)) {
        expect(v.length).toBeGreaterThan(0);
        expect(holes(v), k).toEqual(holes(en[k as keyof typeof en]));
      }
    }
    expect(LANGUAGES.map((l) => l.label)).toEqual(['English', 'Türkçe', 'Русский', 'Қазақша']);
  });

  it('switches to Russian and Kazakh', () => {
    setLang('ru');
    expect(t('nav.settings')).toBe('Настройки');
    expect(document.documentElement.lang).toBe('ru');
    setLang('kk');
    expect(t('nav.settings')).toBe('Баптаулар');
  });

  it('uses the Russian one/few/many forms', () => {
    setLang('ru');
    const n = (k: number) => t('console.lines', { n: k });
    expect([1, 2, 5, 11, 21, 22, 25, 1.5].map(n)).toEqual(['1 строка', '2 строки', '5 строк', '11 строк', '21 строка', '22 строки', '25 строк', '1.5 строки']);
    expect(t('players.onlineCount', { n: 3 })).toBe('3 игрока онлайн');
    expect(t('settings.changes', { n: 1 })).toBe('1 несохранённое изменение');
    setLang('kk');
    expect(t('console.lines', { n: 1 })).toBe('1 жол');
    expect(t('console.lines', { n: 7 })).toBe('7 жол');
  });

  it('detects the language from the OS locale', () => {
    expect(detectLang(['de-DE', 'ru-RU'])).toBe('ru');
    expect(detectLang(['kk_KZ'])).toBe('kk');
    expect(detectLang(['TR'])).toBe('tr');
    expect(detectLang(['fr'])).toBe('en');
    expect(detectLang([])).toBe('en');
    expect(detectLang()).toBe('en');
    const spy = vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['kk-KZ']);
    setLang('');
    expect(i18n.lang).toBe('kk');
    spy.mockReturnValue(undefined as never);
    vi.spyOn(navigator, 'language', 'get').mockReturnValue('ru');
    expect(detectLang()).toBe('ru');
    vi.restoreAllMocks();
  });

  it('formats bytes, percentages and days per language', () => {
    expect(bytes(180 * 2 ** 20)).toBe('180 MB');
    expect(bytes(1.4 * 2 ** 30)).toBe('1.4 GB');
    expect(percent(45)).toBe('45%');
    expect(percent(0.4)).toBe('<1%');
    expect(percent(0)).toBe('0%');
    expect(ago(3 * 864e5)).toBe('3 days ago');
    expect(ago(864e5 + 1000)).toBe('yesterday');
    setLang('tr');
    expect(bytes(1.4 * 2 ** 30)).toBe('1,4 GB');
    expect(percent(45)).toBe('%45');
    expect(percent(0.4)).toBe('%<1');
    setLang('ru');
    expect(bytes(180 * 2 ** 20)).toBe('180 МБ');
    expect(percent(45)).toBe('45\u00a0%');
    expect(ago(3 * 864e5)).toBe('3 дня назад');
    expect(duration(134 * 60000)).toBe('2\u00a0ч 14\u00a0мин');
    expect(ago(6 * 60000)).toBe('6\u00a0мин назад');
    setLang('kk');
    expect(ago(5 * 864e5)).toBe('5 күн бұрын');
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
    expect(duration(134 * 60000)).toBe('2\u00a0sa 14\u00a0dk');
  });
});
