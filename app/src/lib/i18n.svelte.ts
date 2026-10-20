import { en, tr, type Key } from './messages';
import { ru } from './messages.ru';
import { kk } from './messages.kk';

/** The picker lists each language under its own name. */
export const LANGUAGES = [
  { code: 'en', label: 'English' },
  { code: 'tr', label: 'Türkçe' },
  { code: 'ru', label: 'Русский' },
  { code: 'kk', label: 'Қазақша' }
] as const;

const dicts: Record<string, Record<Key, string>> = { en, tr, ru, kk };

/** First supported language among the OS / browser preferences (`ru-RU` → `ru`), else English. */
export function detectLang(prefs: readonly string[] = navigator.languages ?? [navigator.language]): string {
  for (const p of prefs) {
    const base = String(p).toLowerCase().split(/[-_]/)[0];
    if (base in dicts) return base;
  }
  return 'en';
}

export const i18n = $state({ lang: detectLang() });

/** Switch language; an empty or unknown code means "follow the system". */
export function setLang(code: string) {
  i18n.lang = code in dicts ? code : detectLang();
  document.documentElement.lang = i18n.lang;
}

type Params = Record<string, string | number>;

/** Resolve `{n, plural, one {# item} few {…} many {…} other {# items}}` with the language's plural rules. */
function plural(s: string, params: Params): string {
  return s.replace(/\{(\w+), plural,((?:\s*\w+ \{[^{}]*\})+)\s*\}/g, (_, k: string, body: string) => {
    const n = Number(params[k]);
    const cases = Object.fromEntries([...body.matchAll(/(\w+) \{([^{}]*)\}/g)].map((m) => [m[1], m[2]]));
    const rule = new Intl.PluralRules(i18n.lang).select(n);
    return (cases[rule] ?? cases.other).replace(/#/g, String(n));
  });
}

/** Translate; `{name}` placeholders are filled from `params`. Unknown keys fall back to English. */
export function t(key: Key, params: Params = {}): string {
  const s = plural(dicts[i18n.lang]?.[key] ?? en[key] ?? key, params);
  return s.replace(/\{(\w+)\}/g, (_, k) => String(params[k] ?? `{${k}}`));
}

/** Map a backend error code (e.g. "running") to a message. */
export function errText(e: unknown): string {
  const code = String(e);
  const k = `err.${code}` as Key;
  return k in en ? t(k) : code;
}
