<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { ModSummary } from '../types';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import DangerBadge from './DangerBadge.svelte';
  import ModIcon from './ModIcon.svelte';
  import ModStatus from './ModStatus.svelte';
  import { ATTENTION, loc, mods, sidesOf } from './mods.svelte';
  import SidesBadge from './SidesBadge.svelte';
  import { permInfo, permKey } from './perms';

  let { mod, compact = false }: { mod: ModSummary; compact?: boolean } = $props();
  const name = $derived(loc(mod.name, mod.id));
  const att = $derived(ATTENTION.includes(mod.status));
  /** The missing hard dependency, when it is simply not installed (the row offers to install it). */
  const dep = $derived(mod.dependencies.find((d) => !d.ok && !d.soft));
  const perms = $derived(mod.missing.map((p) => t(permKey(permInfo(p), ''), { host: permInfo(p).host ?? '', name: p })).join(', '));
</script>

<li class={['mrow', att && 'att', compact && 'compact']} data-testid="mod-row">
  <ModIcon id={mod.id} {name} size={compact ? 'sm' : 'md'} />
  <div class="mm">
    <div class="mt">
      <button class="nm" onclick={() => mods.open(mod.id)}>{name}</button>
      <span class="mono hint">{mod.version}</span>
      {#if !compact && mod.authors.length}<span class="hint">{t('mods.by', { name: mod.authors.join(', ') })}</span>{/if}
    </div>
    <div class="mb">
      {#if !compact}<SidesBadge sides={sidesOf(mod.sides.server, mod.sides.client)} />{/if}
      <ModStatus status={mod.status} />
      {#if mod.native && !compact}<DangerBadge />{/if}
      {#if mod.update && !compact}<span class="upd"><Icon name="update" size={14} />{t('mods.updateTo', { version: mod.update })}</span>{/if}
    </div>
    {#if !compact}
      {#if mod.status === 'budget'}<p class="hint why">{t('mods.why.budget', { detail: mod.detail ?? '' })}</p>
      {:else if mod.status === 'dependency' && dep}<p class="hint why">{dep.installed ? t('mods.why.depOff', { dep: `${dep.id} ${dep.req}` }) : t('mods.why.depMissing', { dep: `${dep.id} ${dep.req}` })}</p>
      {:else if mod.status === 'permissions'}<p class="hint why">{t('mods.why.perms', { perms })}</p>
      {:else if mod.status === 'failed'}<p class="hint why">{t('mods.why.failed', { detail: mod.detail ?? '' })}</p>
      {:else if mod.status === 'restart'}<p class="hint why">{t('mods.why.restart')}</p>{/if}
    {/if}
  </div>
  {#if !compact}
    {#if mod.status === 'budget'}<Button size="sm" onclick={() => mods.open(mod.id)}>{t('mods.fix.budget')}</Button>
    {:else if mod.status === 'dependency' && dep && !dep.installed}<Button size="sm" icon="download" onclick={() => mods.install(dep.id)}>{t('mods.fix.dep', { name: dep.id })}</Button>
    {:else if mod.status === 'dependency' || mod.status === 'permissions'}<Button size="sm" onclick={() => mods.open(mod.id)}>{t(mod.status === 'permissions' ? 'mods.fix.perms' : 'mods.fix.open')}</Button>
    {:else if mod.status === 'failed'}<Button size="sm" onclick={() => mods.open(mod.id)}>{t('mods.fix.failed')}</Button>{/if}
  {/if}
  <Toggle checked={mod.enabled} label={t('mods.onOff', { name })} hideLabel disabled={mods.busy === mod.id} onchange={(v) => mods.toggle(mod, v)} />
  {#if !compact}<Button size="sm" variant="ghost" icon="chev-r" label={t('mods.details', { name })} onclick={() => mods.open(mod.id)} />{/if}
</li>

<style>
  .mrow { display: flex; align-items: center; gap: 14px; padding: 12px 16px; border-top: 1px solid var(--line); min-height: 72px; }
  .mrow:first-child { border-top: 0; }
  .mrow.att { background: color-mix(in oklab, var(--warn-soft), transparent 40%); }
  .compact { gap: 12px; padding: 10px 12px; min-height: 64px; }
  .mm { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .mt { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }
  .nm {
    border: 0;
    background: none;
    padding: 0;
    color: var(--text);
    font: 700 16px var(--f-body);
    cursor: pointer;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .nm:hover { text-decoration: underline; }
  .compact .nm { font-size: 15px; }
  .mb { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .why { color: var(--text-2); overflow-wrap: anywhere; }
  .upd { display: inline-flex; align-items: center; gap: 5px; font: 700 12.5px var(--f-body); color: var(--primary-ink); white-space: nowrap; }
  @media (max-width: 760px) {
    .mrow:not(.compact) { flex-wrap: wrap; }
    .mrow:not(.compact) .mm { flex-basis: calc(100% - 60px); }
  }
</style>
