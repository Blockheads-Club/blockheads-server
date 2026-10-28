import { api } from '../api';
import { errText, i18n, t } from '../i18n.svelte';
import { en, type Key } from '../messages';
import { store } from '../store.svelte';
import type {
  Browse,
  BrowseMod,
  Installed,
  InstallProgress,
  ModError,
  ModsOverview,
  ModStatus,
  ModSummary,
  Plan
} from '../types';
import { toast } from '../ui/toast.svelte';

/** Statuses that put a mod on the "needs attention" list (warm row, one fix button). */
export const ATTENTION: ModStatus[] = ['failed', 'budget', 'dependency', 'permissions'];

export type ModFilter = 'all' | 'running' | 'off' | 'attention' | 'updates' | 'client';
export type SideFilter = 'all' | 'server' | 'client' | 'both';
export type Sides = 'server' | 'client' | 'both';
export type ModView = 'list' | 'detail' | 'browse';
export type ModTab = 'overview' | 'settings';
export type BrowseSide = 'any' | 'server' | 'client';
export type BrowseSort = 'name' | 'updated' | 'newest';

/** A localized manifest text in the UI language, else English, else any, else `fallback`. */
export function loc(map: Record<string, string>, fallback: string): string {
  return map[i18n.lang] ?? map.en ?? Object.values(map)[0] ?? fallback;
}

export function sidesOf(server: boolean, client: boolean): Sides {
  return server && client ? 'both' : client ? 'client' : 'server';
}

export function isModError(e: unknown): e is ModError {
  return typeof e === 'object' && e !== null && typeof (e as ModError).code === 'string';
}

/** The message for a mod command failure. */
export function modErr(e: unknown): string {
  if (!isModError(e)) return errText(e);
  const k = `modErr.${e.code}` as Key;
  const params = { detail: e.detail ?? '', file: e.file ?? '' };
  return k in en ? t(k, params) : t('modErr.other', { detail: e.detail ?? e.code });
}

/** Install dialog stages (design: InstallFlow). */
export type FlowStage = 'file' | 'checking' | 'plan' | 'perms' | 'native' | 'installing' | 'done' | 'failed';

export interface Flow {
  stage: FlowStage;
  /** Where it started: an index id, or a file / link. */
  origin: { kind: 'index'; id: string; version?: string } | { kind: 'local'; url?: string };
  plan: Plan | null;
  /** Granted permissions per mod (ticked boxes). */
  grants: Record<string, string[]>;
  /** The admin ticked "I trust the author" on the native warning. */
  trust: boolean;
  enable: boolean;
  progress: InstallProgress | null;
  error: ModError | null;
  result: Installed | null;
}

/** Mods screen state; one instance for the whole app. */
export class ModsStore {
  overview = $state.raw<ModsOverview | null>(null);
  /** The overview could not be loaded at all. */
  loadError = $state.raw<ModError | null>(null);
  /** The last action failed. */
  error = $state('');
  view = $state<ModView>('list');
  detailId = $state<string | null>(null);
  /** Section of a mod's page. */
  tab = $state<ModTab>('overview');
  query = $state('');
  filter = $state<ModFilter>('all');
  side = $state<SideFilter>('all');
  busy = $state('');

  browse = $state.raw<Browse | null>(null);
  browseLoading = $state(false);
  browseError = $state.raw<ModError | null>(null);
  bquery = $state('');
  bside = $state<BrowseSide>('any');
  hideNative = $state(false);
  sort = $state<BrowseSort>('updated');
  tag = $state('');

  flow = $state<Flow | null>(null);
  private refreshing: Promise<void> | null = null;

  get mods(): ModSummary[] {
    return this.overview?.mods ?? [];
  }
  get attention(): ModSummary[] {
    return this.mods.filter((m) => ATTENTION.includes(m.status));
  }
  get running(): ModSummary[] {
    return this.mods.filter((m) => m.status === 'running');
  }
  get off(): ModSummary[] {
    return this.mods.filter((m) => !m.enabled);
  }
  get updates(): ModSummary[] {
    return this.mods.filter((m) => m.update);
  }
  /** Mods waiting for the server: a dependency, permissions or a restart. */
  get waiting(): ModSummary[] {
    return this.mods.filter((m) => m.status === 'dependency' || m.status === 'permissions' || m.status === 'restart');
  }
  get detail(): ModSummary | undefined {
    return this.mods.find((m) => m.id === this.detailId);
  }

  /** The installed list after search, status chip and side filter. */
  get visible(): ModSummary[] {
    const q = this.query.trim().toLowerCase();
    const pick: Record<ModFilter, (m: ModSummary) => boolean> = {
      all: () => true,
      running: (m) => m.status === 'running',
      off: (m) => !m.enabled,
      attention: (m) => ATTENTION.includes(m.status),
      updates: (m) => !!m.update,
      client: (m) => this.overview!.clientMods.includes(m.id)
    };
    return this.mods.filter(
      (m) =>
        pick[this.filter](m) &&
        (this.side === 'all' || sidesOf(m.sides.server, m.sides.client) === this.side) &&
        (!q || [loc(m.name, m.id), m.id, ...m.authors].some((s) => s.toLowerCase().includes(q)))
    );
  }

  /** The public list after search, side, native, tag and sort. */
  get browseList(): BrowseMod[] {
    const q = this.bquery.trim().toLowerCase();
    const list = (this.browse?.mods ?? []).filter(
      (m) =>
        (this.bside === 'any' || (this.bside === 'server' ? !m.clientRequired : m.clientRequired)) &&
        !(this.hideNative && m.native) &&
        (!this.tag || m.tags.includes(this.tag)) &&
        (!q || [m.name, m.id, m.description ?? '', ...m.owners, ...m.tags].some((s) => s.toLowerCase().includes(q)))
    );
    const by: Record<BrowseSort, (a: BrowseMod, b: BrowseMod) => number> = {
      name: (a, b) => a.name.localeCompare(b.name),
      updated: (a, b) => b.published.localeCompare(a.published),
      newest: (a, b) => b.firstPublished.localeCompare(a.firstPublished)
    };
    return [...list].sort(by[this.sort]);
  }
  get tags(): string[] {
    return [...new Set((this.browse?.mods ?? []).flatMap((m) => m.tags))].sort();
  }

  /** Reload the installed mods (concurrent calls share one request). */
  refresh(): Promise<void> {
    this.refreshing ??= api
      .modsOverview()
      .then(
        (o) => {
          this.overview = o;
          this.loadError = null;
        },
        (e) => {
          this.loadError = isModError(e) ? e : { code: 'other', detail: String(e) };
        }
      )
      .finally(() => (this.refreshing = null));
    return this.refreshing!;
  }

  open(id: string, tab: ModTab = 'overview') {
    this.detailId = id;
    this.tab = tab;
    this.view = 'detail';
    store.navigate('mods');
  }
  showList(filter?: ModFilter) {
    this.view = 'list';
    if (filter) this.filter = filter;
    store.navigate('mods');
  }
  showBrowse(query = '') {
    this.view = 'browse';
    this.bquery = query;
    store.navigate('mods');
    if (!this.browse) void this.loadBrowse();
  }

  /** Run a mod action; on failure the message is shown. The list is reloaded either way. */
  async act(key: string, fn: () => Promise<unknown>, done?: string): Promise<boolean> {
    this.error = '';
    this.busy = key;
    let ok = true;
    try {
      await fn();
      if (done) toast(done);
    } catch (e) {
      this.error = modErr(e);
      ok = false;
    }
    this.busy = '';
    await this.refresh();
    return ok;
  }

  toggle(m: ModSummary, on: boolean) {
    const name = loc(m.name, m.id);
    return this.act(m.id, () => api.modsSetEnabled(m.id, on), t(on ? 'mods.turnedOn' : 'mods.turnedOff', { name }));
  }
  grant(m: ModSummary, perm: string, on: boolean) {
    return this.act(`${m.id}:${perm}`, () => api.modsSetGrant(m.id, perm, on));
  }
  async remove(m: ModSummary) {
    const ok = await this.act(m.id, () => api.modsRemove(m.file), t('mods.removed', { name: loc(m.name, m.id) }));
    if (ok) this.view = 'list';
    return ok;
  }
  /** A file in the mods folder that is not a valid mod. */
  removeFile(file: string) {
    return this.act(file, () => api.modsRemove(file), t('mods.removed', { name: file }));
  }
  clearData(m: ModSummary) {
    return this.act(m.id, () => api.modsClearData(m.id), t('mods.cleared', { name: loc(m.name, m.id) }));
  }
  openFolder(id?: string) {
    return this.act('folder', () => api.modsOpenFolder(id));
  }

  async loadBrowse() {
    this.browseLoading = true;
    this.browseError = null;
    try {
      this.browse = await api.modsBrowse();
    } catch (e) {
      this.browseError = isModError(e) ? e : { code: 'other', detail: String(e) };
    }
    this.browseLoading = false;
  }

  // ---- install flow ----

  private start(origin: Flow['origin'], stage: FlowStage) {
    this.flow = { stage, origin, plan: null, grants: {}, trust: false, enable: true, progress: null, error: null, result: null };
  }

  private fail(f: Flow, e: unknown) {
    f.error = isModError(e) ? e : { code: 'other', detail: String(e) };
    f.stage = 'failed';
  }

  /** The plan arrived: every requested permission starts ticked, except native (its own confirmation). */
  private planned(f: Flow, plan: Plan) {
    f.plan = plan;
    f.grants = Object.fromEntries(plan.items.map((i) => [i.id, i.permissions.filter((p) => p !== 'native')]));
    f.stage = plan.items.length > 1 || plan.missing.length > 0 || plan.satisfied.length > 0 ? 'plan' : 'perms';
  }

  /** Runs a check step; a dialog closed meanwhile stays closed. */
  private async check(origin: Flow['origin'], get: () => Promise<Plan>) {
    this.start(origin, 'checking');
    const f = this.flow!;
    try {
      const plan = await get();
      if (this.flow === f) this.planned(f, plan);
    } catch (e) {
      if (this.flow === f) this.fail(f, e);
    }
  }

  /** Install (or update) a mod from the public list. */
  install(id: string, version?: string) {
    return this.check({ kind: 'index', id, version }, () => api.modsPlan(id, version));
  }

  /** The "Install from a file or link" dialog. */
  openFile() {
    this.start({ kind: 'local' }, 'file');
  }

  pickFile(file: File) {
    return this.check({ kind: 'local' }, async () => (await api.modsInspectFile(file.name, new Uint8Array(await file.arrayBuffer()))).plan);
  }

  pickUrl(url: string) {
    return this.check({ kind: 'local', url }, async () => (await api.modsInspectUrl(url.trim())).plan);
  }

  /** Try the same thing again after a failure. */
  retry() {
    const o = this.flow!.origin;
    if (o.kind === 'index') return this.install(o.id, o.version);
    if (o.url) return this.pickUrl(o.url);
    this.openFile();
  }

  setGrant(id: string, perm: string, on: boolean) {
    const g = this.flow!.grants[id] ?? [];
    this.flow!.grants[id] = on ? [...g.filter((p) => p !== perm), perm] : g.filter((p) => p !== perm);
  }

  get nativeItems() {
    return this.flow?.plan?.items.filter((i) => i.native) ?? [];
  }

  /** "Install and turn on" / "Install, keep off": native code asks once more. */
  async confirm(enable: boolean) {
    const f = this.flow!;
    f.enable = enable;
    if (this.nativeItems.length && f.stage !== 'native') {
      f.stage = 'native';
      return;
    }
    if (f.stage === 'native') for (const i of this.nativeItems) this.setGrant(i.id, 'native', true);
    await this.run();
  }

  private async run() {
    const f = this.flow!;
    f.stage = 'installing';
    f.progress = null;
    const off = await api.onModsProgress((p) => this.flow && (this.flow.progress = p)).catch(() => undefined);
    try {
      f.result = await api.modsInstall({
        items: f.plan!.items.map((i) => ({ id: i.id, version: i.version })),
        grants: $state.snapshot(f.grants),
        enable: f.enable,
        local: f.origin.kind === 'local'
      });
      f.stage = 'done';
    } catch (e) {
      this.fail(f, e);
    }
    off?.();
    await this.refresh();
    if (this.browse) await this.loadBrowse();
  }

  /** Close the dialog (not while installing: the files are being written). */
  close() {
    if (this.flow?.stage !== 'installing') this.flow = null;
  }
}

export const mods = new ModsStore();
