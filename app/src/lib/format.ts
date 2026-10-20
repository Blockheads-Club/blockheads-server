import { i18n, t } from './i18n.svelte';
import { parts } from './util';

/** `2h 14m` / `14 min` / `under 1 min`: for uptime and "online for". */
export function duration(ms: number): string {
  const { h, m } = parts(ms);
  if (h > 0) return t('time.hm', { h, m });
  if (m > 0) return t('time.m', { m });
  return t('time.lt1');
}

const DAY = 24 * 3600 * 1000;

/** `6 min ago` / `just now`; a day or more reads `yesterday` / `3 days ago` in the current language. */
export function ago(ms: number): string {
  if (ms >= DAY) return new Intl.RelativeTimeFormat(i18n.lang, { numeric: 'auto' }).format(-Math.floor(ms / DAY), 'day');
  const { h, m } = parts(ms);
  return h === 0 && m === 0 ? t('time.now') : t('time.ago', { d: duration(ms) });
}

/** `180 MB` / `1.4 GB` (binary units, as system monitors show memory), with the language's unit names and decimal sign. */
export function bytes(n: number): string {
  const mb = n / 2 ** 20;
  const [v, unit, digits] = mb < 1024 ? [Math.round(mb), 'megabyte', 0] : [mb / 1024, 'gigabyte', 1];
  return new Intl.NumberFormat(i18n.lang, { style: 'unit', unit, unitDisplay: 'short', minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
}

/** `45%` / `%45` (Turkish) / `45 %` (Russian, Kazakh); `<1%` for a small non-zero share. */
export function percent(v: number): string {
  const f = new Intl.NumberFormat(i18n.lang, { style: 'percent', maximumFractionDigits: 0 });
  return v > 0 && v < 1 ? f.format(0.01).replace(/1/, '<1') : f.format(Math.round(v) / 100);
}
