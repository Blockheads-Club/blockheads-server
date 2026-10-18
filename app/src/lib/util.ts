import type { LogLevel } from './types';

/** Classify a raw server log line so the console can colour and filter it. */
export function classify(text: string): LogLevel {
  if (/ - Player (Connected|Disconnected) /.test(text)) return 'join';
  if (/\b(error|failed|fatal|panic|exception)\b/i.test(text)) return 'error';
  if (/\bwarn(ing)?\b/i.test(text)) return 'warn';
  if (/^[\w.-]{1,24}: \S/.test(text)) return 'chat';
  return 'info';
}

export const LEVEL_TAGS: Record<LogLevel, string> = {
  info: 'INFO',
  chat: 'CHAT',
  join: 'JOIN',
  warn: 'WARN',
  error: 'ERROR',
  cmd: 'CMD'
};

/** Clock time `HH:MM:SS` in the local zone. */
export function clock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** Break a duration in ms into whole hours / minutes / seconds. */
export function parts(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000));
  return { h: Math.floor(total / 3600), m: Math.floor((total % 3600) / 60), s: total % 60 };
}

/** Two-letter avatar text for a player name (letters and digits only). */
export function initials(name: string): string {
  const chars = Array.from(name).filter((c) => /[\p{L}\p{N}]/u.test(c));
  return (chars.slice(0, 2).join('') || '?').toLocaleUpperCase();
}

const AVATAR_COLORS = ['#FFD27A', '#F6C79C', '#B9D7F5', '#E8D5F2', '#CDEBD8', '#ECE4DA'];
export function avatarColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)!) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

/** `7c1e5b2a90f` -> `7c1e…90f`; short ids are returned as they are. */
export function shortId(id: string): string {
  return id.length <= 9 ? id : `${id.slice(0, 4)}…${id.slice(-4)}`;
}

/** Copy text to the clipboard; false when the platform refuses. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Run `fn` with a localStorage that may be missing or throw (private windows, blocked storage). */
export function storage<T>(fn: (s: Storage) => T, fallback: T): T {
  try {
    return fn(localStorage);
  } catch {
    return fallback;
  }
}
