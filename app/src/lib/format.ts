import { t } from './i18n.svelte';
import { parts } from './util';

/** `2h 14m` / `14 min` / `under 1 min`: for uptime and "online for". */
export function duration(ms: number): string {
  const { h, m } = parts(ms);
  if (h > 0) return t('time.hm', { h, m });
  if (m > 0) return t('time.m', { m });
  return t('time.lt1');
}

/** `6 min ago` / `just now`. */
export function ago(ms: number): string {
  const { h, m } = parts(ms);
  return h === 0 && m === 0 ? t('time.now') : t('time.ago', { d: duration(ms) });
}

/** `180 MB` / `1.4 GB` (binary units, as system monitors show memory). */
export function bytes(n: number): string {
  const mb = n / 2 ** 20;
  return mb < 1024 ? `${Math.round(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}
