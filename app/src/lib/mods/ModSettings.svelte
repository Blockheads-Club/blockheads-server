<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import type { ModSummary } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import EmptyState from '../ui/EmptyState.svelte';
  import { modErr } from './mods.svelte';
  import Notice from './Notice.svelte';
  import SettingControl from './SettingControl.svelte';
  import { fieldText, modSettings as ms, problemText, same } from './settings.svelte';

  let { mod }: { mod: ModSummary } = $props();

  // Edits survive switching tabs; another mod loads its own settings.
  $effect(() => {
    if (ms.id !== mod.id) void ms.load(mod.id);
  });
  const wide = (type: string) => type === 'matrix' || type === 'list' || type === 'roles';
</script>

<div class="panel">
  {#if ms.loading && !ms.data}
    <p class="hint" role="status">{t('modSettings.loading')}</p>
  {:else if ms.loadError}
    <Notice tone="bad" title={t('modSettings.loadFailed')}>
      <p>{modErr(ms.loadError)}</p>
      {#snippet actions()}<Button size="sm" icon="refresh" onclick={() => ms.load(mod.id)}>{t('modSettings.retry')}</Button>{/snippet}
    </Notice>
  {:else if ms.fields.length === 0}
    <EmptyState icon="sliders" tone="quiet" title={t('modSettings.emptyTitle')} text={t('modSettings.empty')} />
  {:else}
    {#if ms.restartNeeded}
      <Notice tone="info" icon="refresh" title={t('modSettings.restartTitle')}>
        <p>{t('modSettings.restartText')}</p>
        {#snippet actions()}<Button size="sm" variant="primary" icon="refresh" onclick={() => store.restart()}>{t('mods.restartNow')}</Button>{/snippet}
      </Notice>
    {/if}
    {#each ms.groups as g, gi (g.name)}
      <section class="card sec" aria-labelledby={`sg-${gi}`}>
        <h2 id={`sg-${gi}`}>{g.name || t('modSettings.general')}</h2>
        {#each g.fields as f (f.key)}
          {@const label = fieldText(f.label, f.key)}
          {@const help = fieldText(f.help, '')}
          {@const problem = ms.problems[f.key]}
          {@const err = ms.errors[f.key] ?? (problem && !(f.type === 'list' && problem.at) ? problemText(problem) : '')}
          {@const inBox = f.type === 'int' || f.type === 'float' || f.type === 'string'}
          <div class="row" class:wide={wide(f.type)} data-testid="setting" data-key={f.key}>
            <div class="rl">
              <p class="t">
                <span>{label}</span>
                {#if f.restart}<Badge tone="warn" icon="refresh">{t('modSettings.restartBadge')}</Badge>{/if}
                {#if ms.dirty.includes(f.key)}<span class="dot" title={t('modSettings.changed')}><span class="sr">{t('modSettings.changed')}</span></span>{/if}
              </p>
              {#if help}<p class="hint">{help}</p>{/if}
              {#if (f.type === 'int' || f.type === 'float') && (f.min !== undefined || f.max !== undefined)}
                <p class="hint mono">{f.min !== undefined && f.max !== undefined ? t('modSettings.range', { min: f.min, max: f.max }) : f.min !== undefined ? t('modSettings.rangeMin', { min: f.min }) : t('modSettings.rangeMax', { max: f.max! })}</p>
              {/if}
            </div>
            <div class="ctl">
              <SettingControl field={f} value={ms.draft[f.key]} {label} error={inBox && err ? err : undefined} onchange={(v) => ms.set(f.key, v)} />
              {#if !same(ms.draft[f.key], f.default)}
                <button type="button" class="link" onclick={() => ms.reset(f)} aria-label={t('modSettings.resetAria', { name: label })}>{t('modSettings.reset')}</button>
              {/if}
            </div>
            {#if err && !inBox}<p class="err" role="alert">{err}</p>{/if}
          </div>
        {/each}
      </section>
    {/each}
    <div class="bar" class:on={ms.dirty.length > 0} role="region" aria-label={t('modSettings.barAria')}>
      <span class="state">{ms.dirty.length ? t('modSettings.unsaved', { n: ms.dirty.length }) : t('modSettings.allSaved')}</span>
      <div class="acts">
        <Button size="sm" variant="ghost" onclick={() => ms.resetAll()}>{t('modSettings.resetAll')}</Button>
        <Button size="sm" disabled={!ms.dirty.length || ms.saving} onclick={() => ms.discard()}>{t('modSettings.discard')}</Button>
        <Button size="sm" variant="primary" icon="check" loading={ms.saving} disabled={!ms.canSave} onclick={() => ms.save()}>{t('modSettings.save')}</Button>
      </div>
    </div>
  {/if}
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 16px; }
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); }
  .sec { padding: 18px 20px 6px; display: flex; flex-direction: column; min-width: 0; }
  h2 { font-size: 19px; margin-bottom: 4px; }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px 16px;
    padding: 14px 0;
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
  h2 + .row { border-top: 0; }
  .row.wide { flex-direction: column; align-items: stretch; }
  .rl { flex: 1 1 260px; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .t { font-weight: 700; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--primary); flex: none; }
  .ctl { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; max-width: 100%; min-width: 0; }
  .row.wide .rl { flex: none; }
  .row.wide .ctl { align-items: flex-start; flex-direction: column; }
  .link { border: 0; background: none; color: var(--primary-ink); font: 700 13.5px var(--f-body); text-decoration: underline; cursor: pointer; padding: 0; min-height: 32px; }
  .err { flex-basis: 100%; font-size: 13.5px; color: var(--bad); font-weight: 700; }
  .bar {
    position: sticky;
    bottom: 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    padding: 10px 14px;
    background: var(--surface);
    border: 2px solid var(--line);
    border-radius: var(--r-lg);
    box-shadow: 0 2px 0 var(--line-strong);
  }
  .bar.on { border-color: var(--primary-edge); }
  .state { font-weight: 700; }
  .bar:not(.on) .state { color: var(--text-3); }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; }
  @media (max-width: 599px) {
    .sec { padding: 14px 14px 4px; }
    .bar { bottom: 8px; }
  }
</style>
