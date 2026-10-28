import { api } from '../api';
import { t } from '../i18n.svelte';
import { en, type Key } from '../messages';
import { store } from '../store.svelte';
import type { Json, ModError, ModSettings, SettingField, SettingFieldBase } from '../types';
import { toast } from '../ui/toast.svelte';
import { isModError, loc, modErr } from './mods.svelte';

/** Roles every server has (vanilla lists); a mod's values may add groups of a permissions mod. */
export const BUILTIN_ROLES = ['owner', 'admin', 'mod', 'everyone'] as const;

/** A broken rule, in the shape the backend reports it (`setting` errors: detail = rule, expected, got = where). */
export interface Problem {
  rule: string;
  expected?: string;
  at?: string;
}

/** Deep equality of JSON values (object key order does not matter). */
export function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null || Array.isArray(a) !== Array.isArray(b)) return false;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  const kx = Object.keys(x);
  return kx.length === Object.keys(y).length && kx.every((k) => k in y && same(x[k], y[k]));
}

export function clone<T extends Json>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

/** The label (or help) in the UI language, else English, else any, else `fallback`. */
export function fieldText(map: Record<string, string> | undefined, fallback: string): string {
  return map ? loc(map, fallback) : fallback;
}

/** List item fields, in declaration order. */
export function itemFields(f: SettingFieldBase): SettingFieldBase[] {
  return f.item ?? [];
}

/** A new list item: every sub-field at its default. */
export function newItem(f: SettingFieldBase): Record<string, Json> {
  return Object.fromEntries(itemFields(f).map((s) => [s.key, clone(s.default)]));
}

/** Roles to offer: the built-in ones, then any other names in `extra`. */
export function roleOptions(...extra: Json[]): string[] {
  const names = extra.flatMap((v) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []));
  return [...new Set<string>([...BUILTIN_ROLES, ...names])];
}

/** Shown name of a role: the built-in ones are translated. */
export function roleName(r: string): string {
  return (BUILTIN_ROLES as readonly string[]).includes(r) ? t(`modSettings.role.${r}` as Key) : r;
}

/** Every role name in a matrix of role cells (to offer them in each cell). */
export function matrixRoles(v: Json): Json[] {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return [];
  return Object.values(v).flatMap((row) => (row && typeof row === 'object' && !Array.isArray(row) ? Object.values(row) : []));
}

/** Matrix value with one cell changed (a matrix value has every row and column). */
export function setCell(v: Json, row: string, col: string, cell: Json): Record<string, Record<string, Json>> {
  const m = clone((v && typeof v === 'object' && !Array.isArray(v) ? v : {}) as Record<string, Record<string, Json>>);
  (m[row] ??= {})[col] = cell;
  return m;
}

/** A number as the backend writes limits (`32`, `0.5`). */
function num(n: number): string {
  return String(n);
}

const isObj = (v: Json): v is { [k: string]: Json } => typeof v === 'object' && v !== null && !Array.isArray(v);
const join = (at: string, part: string) => (at ? `${at}.${part}` : part);

/** A role name: `[a-z0-9_-]{1,40}` (bhmod-manifest `is_role`). */
const ROLE = /^[a-z0-9_-]{1,40}$/;

function roles(v: Json, at: string): Problem | null {
  if (!Array.isArray(v)) return { rule: 'type', at };
  const bad = v.some((r, i) => typeof r !== 'string' || !ROLE.test(r) || v.indexOf(r) !== i);
  return bad ? { rule: 'role', at } : null;
}

/** Checks a value against its field with the backend's rules (bhmod-manifest `validate_value`); null when it is fine. */
export function validate(f: SettingFieldBase, v: Json, at = ''): Problem | null {
  const bounds = (n: number): Problem | null =>
    f.min !== undefined && n < f.min
      ? { rule: 'min', expected: num(f.min), at }
      : f.max !== undefined && n > f.max
        ? { rule: 'max', expected: num(f.max), at }
        : null;
  switch (f.type) {
    case 'bool':
      return typeof v === 'boolean' ? null : { rule: 'type', at };
    case 'int': {
      if (typeof v !== 'number' || !Number.isFinite(v)) return { rule: 'number', at };
      if (!Number.isInteger(v)) return { rule: 'integer', at };
      return bounds(v);
    }
    case 'float':
      return typeof v !== 'number' || !Number.isFinite(v) ? { rule: 'number', at } : bounds(v);
    case 'enum':
      return typeof v === 'string' && (f.values ?? []).includes(v) ? null : { rule: 'choice', expected: (f.values ?? []).join(', '), at };
    case 'string':
      if (typeof v !== 'string') return { rule: 'type', at };
      return f.max_length !== undefined && [...v].length > f.max_length ? { rule: 'length', expected: String(f.max_length), at } : null;
    case 'roles':
      return roles(v, at);
    case 'matrix': {
      if (!isObj(v)) return { rule: 'type', at };
      for (const [r, cols] of Object.entries(v)) {
        const rat = join(at, r);
        if (!(f.rows ?? []).includes(r)) return { rule: 'unknown', at: rat };
        if (!isObj(cols)) return { rule: 'type', at: rat };
        for (const [c, cell] of Object.entries(cols)) {
          const cat = join(rat, c);
          if (!(f.cols ?? []).includes(c)) return { rule: 'unknown', at: cat };
          const p = f.cell === 'roles' ? roles(cell, cat) : typeof cell === 'boolean' ? null : { rule: 'type', at: cat };
          if (p) return p;
        }
      }
      return null;
    }
    case 'list': {
      if (!Array.isArray(v)) return { rule: 'type', at };
      if (f.max_items !== undefined && v.length > f.max_items) return { rule: 'items', expected: String(f.max_items), at };
      const subs = itemFields(f);
      for (const [i, it] of v.entries()) {
        const iat = `${at || f.key}[${i}]`;
        if (!isObj(it)) return { rule: 'type', at: iat };
        for (const [k, x] of Object.entries(it)) {
          const sf = subs.find((s) => s.key === k);
          const p = sf ? validate(sf, x, join(iat, k)) : { rule: 'unknown', at: join(iat, k) };
          if (p) return p;
        }
      }
      return null;
    }
  }
}

/** The message of a broken rule; `top` leaves out where inside the value it is (shown next to the sub-field). */
export function problemText(p: Problem, top = false): string {
  const k = `modSettings.err.${p.rule}` as Key;
  const text = k in en ? t(k, { n: p.expected ?? '' }) : t('modSettings.err.other');
  return p.at && !top ? t('modSettings.errAt', { at: p.at, text }) : text;
}

/**
 * The message of a failed save of setting `key`. The backend names where the value broke from the key on
 * (`radius`, `talk.global.owner`, `channels[1].name`); the key itself is left out like in {@link validate}.
 */
export function settingErr(e: unknown, key = ''): string {
  if (isModError(e) && e.code === 'setting') {
    const got = e.got ?? '';
    const at = got === key ? '' : key && got.startsWith(`${key}.`) ? got.slice(key.length + 1) : got;
    return problemText({ rule: e.detail ?? '', expected: e.expected, at: at || undefined });
  }
  return modErr(e);
}

/** The Settings tab of one mod: the schema, the saved values and the admin's unsaved edits. */
export class SettingsStore {
  id = $state<string | null>(null);
  data = $state.raw<ModSettings | null>(null);
  loading = $state(false);
  loadError = $state.raw<ModError | null>(null);
  saved = $state<Record<string, Json>>({});
  draft = $state<Record<string, Json>>({});
  /** Messages from the last save, per key. */
  errors = $state<Record<string, string>>({});
  saving = $state(false);
  /** A setting marked "restart" was saved while the server runs. */
  restartNeeded = $state(false);

  /** Server settings (client ones are each player's own). */
  get fields(): SettingField[] {
    return (this.data?.fields ?? []).filter((f) => f.scope === 'server');
  }
  /** Fields by group in the order they first appear; fields without a group are "General". */
  get groups(): { name: string; fields: SettingField[] }[] {
    const out: { name: string; fields: SettingField[] }[] = [];
    for (const f of this.fields) {
      const name = f.group ?? '';
      const g = out.find((x) => x.name === name);
      if (g) g.fields.push(f);
      else out.push({ name, fields: [f] });
    }
    return out;
  }
  get dirty(): string[] {
    return this.fields.filter((f) => !same(this.draft[f.key], this.saved[f.key])).map((f) => f.key);
  }
  /** Rule problems of the current edits, per key. */
  get problems(): Record<string, Problem> {
    const out: Record<string, Problem> = {};
    for (const f of this.fields) {
      const p = validate(f, this.draft[f.key]);
      if (p) out[f.key] = p;
    }
    return out;
  }
  get canSave(): boolean {
    return !this.saving && this.dirty.length > 0 && this.dirty.every((k) => !this.problems[k]);
  }

  async load(id: string) {
    this.id = id;
    this.loading = true;
    this.loadError = null;
    this.errors = {};
    this.restartNeeded = false;
    try {
      const data = await api.modSettingsGet(id);
      if (this.id !== id) return;
      this.data = data;
      this.saved = Object.fromEntries(data.fields.map((f) => [f.key, clone(f.key in data.values ? data.values[f.key] : f.default)]));
      this.draft = clone(this.saved);
    } catch (e) {
      if (this.id !== id) return;
      this.data = null;
      this.loadError = isModError(e) ? e : { code: 'other', detail: String(e) };
    }
    this.loading = false;
  }

  set(key: string, value: Json) {
    this.draft[key] = value;
    delete this.errors[key];
  }
  reset(f: SettingField) {
    this.set(f.key, clone(f.default));
  }
  resetAll() {
    for (const f of this.fields) this.reset(f);
  }
  discard() {
    this.draft = clone(this.saved);
    this.errors = {};
  }

  /** Saves the changed settings one by one; refused ones keep their edit and show the reason. */
  async save() {
    const id = this.id!;
    this.saving = true;
    let ok = 0;
    for (const f of this.fields.filter((f) => this.dirty.includes(f.key))) {
      const value = clone(this.draft[f.key]);
      try {
        await api.modSettingsSet(id, f.key, value);
        this.saved[f.key] = clone(value);
        ok++;
        if (f.restart && store.active) this.restartNeeded = true;
      } catch (e) {
        this.errors[f.key] = settingErr(e, f.key);
      }
    }
    this.saving = false;
    if (Object.keys(this.errors).length) toast(t('modSettings.notSaved'));
    else if (ok) toast(t('modSettings.saved'));
  }
}

export const modSettings = new SettingsStore();
