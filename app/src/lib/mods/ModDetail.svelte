<script lang="ts">
  import { fileSize } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import type { ModSummary } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Tabs from '../ui/Tabs.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { toast } from '../ui/toast.svelte';
  import { clock, copyText } from '../util';
  import { viewport } from '../viewport.svelte';
  import BudgetMeter from './BudgetMeter.svelte';
  import DangerBadge from './DangerBadge.svelte';
  import DangerBlock from './DangerBlock.svelte';
  import ModIcon from './ModIcon.svelte';
  import ModSettings from './ModSettings.svelte';
  import ModStatus from './ModStatus.svelte';
  import { loc, mods, sidesOf } from './mods.svelte';
  import Notice from './Notice.svelte';
  import PermissionGrantRow from './PermissionGrantRow.svelte';
  import SidesBadge from './SidesBadge.svelte';

  let { mod }: { mod: ModSummary } = $props();
  const name = $derived(loc(mod.name, mod.id));
  const desc = $derived(loc(mod.description, ''));
  const ov = $derived(mods.overview!);
  const desktop = $derived(store.info?.platform !== 'android');
  const dep = $derived(mod.dependencies.find((d) => !d.ok && !d.soft));
  const measuring = $derived(mod.status === 'running' || mod.slowestMs !== null);
  let asking = $state<'remove' | 'clear' | null>(null);
  const tab = $derived(mod.settings > 0 ? mods.tab : 'overview');

  async function copy(text: string) {
    toast((await copyText(text)) ? t('install.copied') : t('qr.copyFailed'));
  }
  const report = $derived(
    [`${mod.id} ${mod.version}: ${mod.detail ?? mod.status}`, ...mod.faults.map((f) => `${clock(f.at)} ${f.kind} ${f.text}`)].join('\n')
  );
  /** The host loads mods when the world starts: reloading one is a server restart. */
  function reload() {
    if (store.active) void store.restart();
    else void mods.refresh();
  }
  function openLog() {
    store.consoleQuery = `${mod.id}:`;
    store.navigate('console');
  }
  async function confirmAsk() {
    const what = asking;
    asking = null;
    if (what === 'remove') await mods.remove(mod);
    else await mods.clearData(mod);
  }
</script>

<div class="main">
  <div class="top">
    <button class="back" onclick={() => mods.showList()}><Icon name="chev-l" size={18} />{t('mods.allMods')}</button>
    <div class="head">
      <ModIcon id={mod.id} {name} size={viewport.phone ? 'md' : 'lg'} />
      <div class="ht">
        <h1>{`${name} `}<span class="mono ver">{mod.version}</span></h1>
        <div class="badges">
          <SidesBadge sides={sidesOf(mod.sides.server, mod.sides.client)} />
          <ModStatus status={mod.status} />
          {#if mod.native}<DangerBadge />{/if}
          {#if mod.authors.length}<span class="hint">{t('mods.by', { name: mod.authors.join(', ') })}</span>{/if}
          {#if mod.reviewed}<span class="rev"><Icon name="shield" size={14} />{t('mods.reviewed')}</span><span class="hint">{t('mods.fromList')}</span>
          {:else}<span class="ext"><Icon name="alert" size={14} />{t('mods.notListed')}</span>{/if}
        </div>
      </div>
      <div class="ctl">
        <Toggle checked={mod.enabled} label={mod.enabled ? t('mods.on') : t('mods.off')} disabled={mods.busy === mod.id} onchange={(v) => mods.toggle(mod, v)} />
        <Button size="sm" icon="refresh" onclick={reload}>{t('mods.reload')}</Button>
        {#if desktop}<Button size="sm" icon="folder" onclick={() => mods.openFolder(mod.id)}>{t('mods.openModFolder')}</Button>{/if}
        <Button size="sm" variant="quiet-bad" icon="trash" onclick={() => (asking = 'remove')}>{t('mods.remove')}</Button>
      </div>
    </div>
  </div>

  {#if mods.error}
    <Notice tone="bad" title={mods.error}>
      {#snippet actions()}<Button size="sm" variant="ghost" onclick={() => (mods.error = '')}>{t('common.close')}</Button>{/snippet}
    </Notice>
  {/if}
  {#if mod.native}
    <DangerBlock>
      <p>{t('mods.nativeDetail', { name })}{#if !mod.reviewed} {t('mods.nativeNotListed')}{/if}</p>
    </DangerBlock>
  {/if}
  {#if mod.status === 'budget'}
    <Notice tone="bad" title={t('mods.budgetTitle')}>
      <p>{t('mods.budgetText', { detail: mod.detail ?? '' })}</p>
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="refresh" onclick={() => store.restart()}>{t('mods.turnOnAgain')}</Button>
        <Button size="sm" onclick={() => copy(report)}>{t('mods.copyReport')}</Button>
      {/snippet}
    </Notice>
  {:else if mod.status === 'failed'}
    <Notice tone="bad" title={t('mods.failedTitle')}>
      <p class="mono">{mod.detail ?? ''}</p>
      <p>{t('mods.failedText')}</p>
      {#snippet actions()}
        <Button size="sm" icon="refresh" onclick={reload}>{t('mods.reload')}</Button>
        <Button size="sm" onclick={() => copy(report)}>{t('mods.copyError')}</Button>
      {/snippet}
    </Notice>
  {:else if mod.status === 'dependency' && dep}
    <Notice tone="info" icon="link" title={t('mods.waitingFor', { name: dep.id })}>
      <p>{t('mods.waitingForText', { name, dep: `${dep.id} ${dep.req}` })}</p>
      {#snippet actions()}{#if !dep.installed}<Button size="sm" variant="primary" icon="download" onclick={() => mods.install(dep.id)}>{t('mods.fix.dep', { name: dep.id })}</Button>{/if}{/snippet}
    </Notice>
  {:else if mod.status === 'permissions'}
    <Notice tone="warn" icon="key" title={t('mods.waitingPerm')}>{t('mods.waitingPermText')}</Notice>
  {:else if mod.status === 'restart'}
    <Notice tone="info" icon="refresh" title={t('mods.why.restart')}>
      {#snippet actions()}<Button size="sm" variant="primary" icon="refresh" onclick={() => store.restart()}>{t('mods.restartNow')}</Button>{/snippet}
    </Notice>
  {/if}
  {#if mod.update}
    <Notice tone="info" icon="update" title={t('mods.updateAvailable', { version: mod.update })}>
      {#snippet actions()}<Button size="sm" variant="primary" icon="download" onclick={() => mods.install(mod.id)}>{t('mods.updateTo', { version: mod.update! })}</Button>{/snippet}
    </Notice>
  {/if}

  {#if mod.settings > 0}
    <div class="tabbar"><Tabs
      label={t('mods.tabsAria')}
      value={tab}
      items={[
        { id: 'overview', label: t('mods.tab.overview'), icon: 'info' },
        { id: 'settings', label: t('mods.tab.settings'), icon: 'sliders' }
      ]}
      onchange={(v) => (mods.tab = v)}
    /></div>
  {/if}

  {#if tab === 'settings'}
    <div role="tabpanel" aria-label={t('mods.tab.settings')}><ModSettings {mod} /></div>
  {:else}
  <div class="cols">
    <div class="col">
      <section class="card sec" aria-labelledby="ab">
        <h2 id="ab">{t('mods.about')}</h2>
        {#if desc}<p>{desc}</p>{/if}
        <dl class="kv">
          <dt>{t('mods.author')}</dt><dd>{mod.authors.join(', ') || '—'}</dd>
          <dt>{t('mods.license')}</dt><dd>{mod.license}</dd>
          <dt>{t('mods.homepage')}</dt><dd>{#if mod.homepage}<a href={mod.homepage} target="_blank" rel="noopener noreferrer">{mod.homepage.replace(/^https?:\/\//, '')}</a>{:else}—{/if}</dd>
          <dt>{t('mods.version')}</dt><dd class="mono">{mod.version}</dd>
          <dt>{t('mods.file')}</dt><dd class="mono">{`${mod.file} · ${fileSize(mod.size)}`}</dd>
          {#if mod.runtime}<dt>{t('mods.runtime')}</dt><dd class="mono">{mod.runtime}</dd>{/if}
        </dl>
      </section>
      <section class="card sec" aria-labelledby="pm">
        <div class="sh"><h2 id="pm">{t('mods.permissions')}</h2><span class="hint">{t('mods.permissionsHint')}</span></div>
        {#if mod.permissions.length}
          <ul class="plain">
            {#each mod.permissions as p (p)}
              <PermissionGrantRow
                perm={p}
                granted={mod.granted.includes(p) || (p === 'world:read' && mod.granted.includes('world:write'))}
                busy={mods.busy === `${mod.id}:${p}`}
                compact={viewport.phone}
                ongrant={() => mods.grant(mod, p, true)}
                onrevoke={() => mods.grant(mod, p, false)}
              />
            {/each}
          </ul>
        {:else}
          <p class="hint">{t('mods.noPermissions')}</p>
        {/if}
      </section>
      {#if mod.sides.client}
        <section class="card sec" aria-labelledby="cr">
          <h2 id="cr"><Icon name="monitor" />{` ${t('mods.clientRequired')}`}</h2>
          {#if mod.sides.clientRequiredLocked}
            <p class="hint">{t('mods.clientRequiredLocked')}</p>
          {:else}
            <Toggle
              checked={mod.sides.clientRequired}
              label={mod.sides.clientRequired ? t('mods.clientRequiredOn') : t('mods.clientRequiredOff')}
              disabled={mods.busy === `${mod.id}:clientRequired`}
              onchange={(v) => mods.setClientRequired(mod, v)}
            />
            <p class="hint">{t('mods.clientRequiredHint', { value: mod.sides.clientRequiredDefault ? t('mods.clientRequiredOn') : t('mods.clientRequiredOff') })}</p>
          {/if}
        </section>
      {/if}
      <section class="card sec" aria-labelledby="cm">
        <h2 id="cm">{t('mods.commands')}</h2>
        <p class="hint">{t('mods.commandsHint')}</p>
      </section>
    </div>
    <div class="col">
      <section class="card sec" aria-labelledby="pf">
        <h2 id="pf"><Icon name="gauge" /> {t('mods.performance')}</h2>
        {#if mod.native}
          <p class="hint">{t('mods.perfNative')}</p>
        {:else if measuring}
          <div class="mrow"><span>{t('mods.slowest')}</span><span class="mono">{(mod.slowestMs ?? 0).toFixed(1)} ms <span class="hint">{t('mods.ofBudget', { budget: ov.softMs.toFixed(1) })}</span></span></div>
          <BudgetMeter usedMs={mod.slowestMs ?? 0} budgetMs={ov.softMs} maxMs={ov.hardMs} label={t('mods.slowest')} />
          <p class="hint">{mod.slowestMs === null ? t('mods.noSlow', { budget: ov.softMs }) : mod.slowestMs > ov.softMs ? t('mods.overBudget') : t('mods.meterHint')}</p>
        {:else}
          <p class="hint">{t('mods.perfIdle')}</p>
        {/if}
        <p class="strong">{t('mods.faults')}</p>
        {#if mod.faults.length}
          <ul class="faults">
            {#each mod.faults.slice().reverse() as f, i (i)}
              <li><span class="mono hint">{clock(f.at)}</span><span>{t(`mods.fault.${f.kind}`, { text: f.text })}</span></li>
            {/each}
          </ul>
        {:else}
          <p class="hint">{t('mods.noFaults')}</p>
        {/if}
      </section>
      <section class="card sec" aria-labelledby="dp">
        <h2 id="dp">{t('mods.dependencies')}</h2>
        {#if mod.dependencies.length}
          <ul class="plain">
            {#each mod.dependencies as d (d.id)}
              <li class="dep">
                <ModIcon id={d.id} name={d.id} size="sm" />
                <p class="grow">{`${d.id} `}<span class="mono hint">{d.req}</span>{#if d.soft} <span class="hint">{t('mods.optional')}</span>{/if}</p>
                {#if d.ok}<Badge tone="ok" icon="check">{t('mods.depInstalled')}</Badge>
                {:else if d.installed}<Badge tone="warn" icon="alert">{t('mods.depOff', { version: d.installed })}</Badge>
                {:else}<Badge tone={d.soft ? 'neutral' : 'bad'} icon="x-circle">{t('mods.depMissing')}</Badge><Button size="sm" variant="primary" onclick={() => mods.install(d.id)}>{t('mods.install')}</Button>{/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="hint">{t('mods.noDependencies')}</p>
        {/if}
      </section>
      <section class="card sec row" aria-labelledby="sg">
        <div class="grow">
          <h2 id="sg"><Icon name="db" /> {t('mods.storage')}</h2>
          <p class="hint">{mod.storageBytes === null ? t('mods.storageRunning') : fileSize(mod.storageBytes)}</p>
        </div>
        <Button size="sm" variant="ghost" disabled={store.active} onclick={() => (asking = 'clear')}>{t('mods.clearData')}</Button>
      </section>
      <section class="card sec" aria-labelledby="lg">
        <div class="sh"><h2 id="lg">{t('mods.log')}</h2><button class="link" onclick={openLog}>{t('mods.openInConsole')}</button></div>
        {#if mod.lines.length}
          <pre class="log" role="log" aria-label={t('mods.logAria', { name })}>{#each mod.lines as l, i (i)}<span class={l.level}>{`${clock(l.at)}  ${l.level.padEnd(5)} ${l.text}`}</span>
{/each}</pre>
        {:else}
          <p class="hint">{store.active ? t('mods.logEmpty') : t('mods.logStopped')}</p>
        {/if}
      </section>
    </div>
  </div>
  {/if}
</div>

{#if asking}
  <Dialog
    title={asking === 'remove' ? t('mods.removeTitle', { name }) : t('mods.clearTitle', { name })}
    icon="trash"
    tone="bad"
    description={asking === 'remove' ? t('mods.removeText') : t('mods.clearText')}
    onclose={() => (asking = null)}
  >
    {#snippet footer()}
      <Button onclick={() => (asking = null)}>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" onclick={confirmAsk}>{asking === 'remove' ? t('mods.remove') : t('mods.clearConfirm')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .main { padding: 20px 28px 32px; display: flex; flex-direction: column; gap: 16px; }
  .top { display: flex; flex-direction: column; gap: 10px; }
  .back {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: none;
    color: var(--primary-ink);
    font: 700 14.5px var(--f-body);
    min-height: 40px;
    cursor: pointer;
    padding: 0;
  }
  .head { display: flex; gap: 18px; align-items: center; flex-wrap: wrap; }
  .ht { flex: 1 1 320px; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  h1 { font-size: 28px; overflow-wrap: anywhere; }
  .ver { font-size: 16px; font-weight: 400; color: var(--text-3); }
  .badges { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .ctl { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .rev { display: inline-flex; align-items: center; gap: 5px; font: 700 12.5px var(--f-body); color: var(--ok); }
  .ext {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px 2px 7px;
    border-radius: 7px;
    font: 700 12.5px/1.5 var(--f-body);
    background: var(--warn-soft);
    color: var(--warn);
    border: 1.5px dashed color-mix(in oklab, var(--warn), transparent 40%);
  }
  .tabbar { align-self: flex-start; max-width: 100%; }
  .cols { display: grid; grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr); gap: 16px; align-items: start; }
  .col { display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); }
  .sec { padding: 18px 20px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .sec.row { flex-direction: row; align-items: center; gap: 12px; }
  h2 { font-size: 19px; display: flex; align-items: center; gap: 8px; }
  .sh { display: flex; justify-content: space-between; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .kv { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 6px 14px; font-size: 14.5px; margin: 0; }
  .kv dt { color: var(--text-3); font-weight: 700; }
  .kv dd { margin: 0; overflow-wrap: anywhere; }
  .kv a { color: var(--primary-ink); }
  .plain { list-style: none; margin: 0; padding: 0; }
  .mrow { display: flex; justify-content: space-between; gap: 10px; font-weight: 700; }
  .strong { font-weight: 700; }
  .faults { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; font-size: 14px; }
  .faults li { display: flex; gap: 10px; overflow-wrap: anywhere; }
  .dep { display: flex; gap: 10px; align-items: center; padding: 8px 0; border-top: 1px solid var(--line); flex-wrap: wrap; }
  .dep:first-child { border-top: 0; }
  .grow { flex: 1; min-width: 0; }
  .link { border: 0; background: none; color: var(--primary-ink); font: 700 14px var(--f-body); text-decoration: underline; cursor: pointer; padding: 0; }
  .log {
    background: var(--code-bg);
    color: var(--code-text);
    font: 12.5px/1.7 var(--f-mono);
    padding: 10px 12px;
    border-radius: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 0;
    max-height: 260px;
    overflow: auto;
  }
  .log .warn { color: #f2c14e; }
  .log .error { color: #ff8f80; }
  @media (max-width: 900px) {
    .cols { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 599px) {
    .main { padding: 6px 14px 20px; gap: 10px; }
    h1 { font-size: 22px; }
    .sec { padding: 14px; }
  }
</style>
