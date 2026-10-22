<script lang="ts">
  import { fileSize } from '../format';
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import { viewport } from '../viewport.svelte';
  import Chip from './Chip.svelte';
  import ModRow from './ModRow.svelte';
  import { loc, mods, modErr, type ModFilter, type SideFilter } from './mods.svelte';
  import Notice from './Notice.svelte';

  const ov = $derived(mods.overview);
  const desktop = $derived(store.info?.platform !== 'android');
  const list = $derived(mods.visible);
  const nameOf = (id: string) => {
    const m = mods.mods.find((x) => x.id === id);
    return m ? loc(m.name, id) : id;
  };
  /** "Later" hides the restart notice until the set of waiting changes is different. */
  let later = $state('');
  const changesKey = $derived(JSON.stringify(ov!.changes));
  const showRestart = $derived(!!ov && ov.running && ov.changes.length > 0 && later !== changesKey);
  const summary = $derived.by(() => {
    if (!ov) return t('mods.subtitle');
    if (!ov.mods.length) return t('mods.subtitleEmpty');
    const parts = [t('mods.sumInstalled', { n: ov.mods.length }), t('mods.sumRunning', { n: mods.running.length })];
    if (mods.attention.length) parts.push(t('mods.sumAttention', { n: mods.attention.length }));
    return parts.join(' · ');
  });
  const chips: { id: ModFilter; icon?: 'alert' | 'update'; count: () => number }[] = [
    { id: 'all', count: () => mods.mods.length },
    { id: 'running', count: () => mods.running.length },
    { id: 'off', count: () => mods.off.length },
    { id: 'attention', icon: 'alert', count: () => mods.attention.length },
    { id: 'updates', icon: 'update', count: () => mods.updates.length }
  ];
  const sides: SideFilter[] = ['all', 'server', 'client', 'both'];
</script>

<PageHeader title={t('nav.mods')} subtitle={summary}>
  {#snippet actions()}
    {#if !viewport.phone}
      <Button icon="link" onclick={() => mods.openFile()}>{t('mods.fromFile')}</Button>
      <Button variant="primary" icon="search" onclick={() => mods.showBrowse()}>{t('mods.browse')}</Button>
    {/if}
  {/snippet}
</PageHeader>

<div class="main">
  {#if mods.error}
    <Notice tone="bad" title={mods.error}>
      {#snippet actions()}<Button size="sm" variant="ghost" onclick={() => (mods.error = '')}>{t('common.close')}</Button>{/snippet}
    </Notice>
  {/if}
  {#if ov && ov.clientMods.length}
    <Notice tone="warn" title={t('mods.loaderTitle')}>
      {#if !viewport.phone}<p>{t('mods.loaderText', { names: ov.clientMods.map(nameOf).join(', ') })}</p>{:else}<p>{t('mods.loaderShort', { names: ov.clientMods.map(nameOf).join(', ') })}</p>{/if}
      {#snippet actions()}
        <details class="how">
          <summary>{t('mods.loaderHow')}</summary>
          <ul>
            <li><strong>{t('mods.loaderAndroid')}</strong> {t('mods.loaderAndroidText')}</li>
            <li><strong>{t('mods.loaderPc')}</strong> {t('mods.loaderPcText')}</li>
          </ul>
        </details>
        {#if !viewport.phone}<Button size="sm" onclick={() => (mods.filter = 'client')}>{t('mods.showClient', { n: ov.clientMods.length })}</Button>{/if}
      {/snippet}
    </Notice>
  {/if}
  {#if showRestart && ov}
    <Notice tone="info" icon="refresh" title={t('mods.restartTitle', { n: ov.changes.length })}>
      <p>{ov.changes.map((c) => t(`mods.change.${c.kind}`, { name: nameOf(c.id), version: c.version })).join(' ')}</p>
      {#if store.players.length}<p>{t('mods.restartPlayers', { n: store.players.length })}</p>{/if}
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="refresh" onclick={() => store.restart()}>{t('mods.restartNow')}</Button>
        <Button size="sm" pressed={store.restartWhenEmpty} onclick={() => (store.restartWhenEmpty = !store.restartWhenEmpty)}>{t('mods.restartEmpty')}</Button>
        <Button size="sm" variant="ghost" onclick={() => (later = changesKey)}>{t('mods.later')}</Button>
      {/snippet}
    </Notice>
  {/if}
  {#if mods.loadError || ov?.error}
    {@const e = mods.loadError ?? ov!.error!}
    <Notice tone="bad" title={e.code === 'config' ? t('mods.configError') : t('mods.folderError')}>
      <p class="mono path">{`${e.file ?? ov?.dir ?? ''}${e.file || ov ? ' — ' : ''}${modErr(e)}`}</p>
      <p>{e.code === 'config' ? t('mods.configErrorText') : t('mods.folderErrorText')}</p>
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="refresh" onclick={() => mods.refresh()}>{t('mods.tryAgain')}</Button>
        {#if desktop}<Button size="sm" icon="folder" onclick={() => mods.openFolder()}>{t('mods.openFolder')}</Button>{/if}
      {/snippet}
    </Notice>
  {/if}
  {#if ov && ov.problems.length}
    <Notice tone="warn" title={t('mods.problems', { n: ov.problems.length })}>
      <ul class="probs">
        {#each ov.problems as p (p.file)}
          <li><span class="mono">{p.file}</span>{` — ${p.message}`}
            <Button size="sm" variant="quiet-bad" icon="trash" onclick={() => mods.removeFile(p.file)}>{t('mods.removeFile')}</Button>
          </li>
        {/each}
      </ul>
    </Notice>
  {/if}

  {#if !ov && !mods.loadError}
    <ul class="card rows" aria-busy="true">
      <li class="sk"><span class="skel ic"></span><span class="col"><span class="skel l1"></span><span class="skel l2"></span></span></li>
      <li class="sk"><span class="skel ic"></span><span class="col"><span class="skel l1 b"></span><span class="skel l2 b"></span></span></li>
      <li class="sk"><span class="skel ic"></span><span class="col"><span class="skel l1 c"></span><span class="skel l2 c"></span></span></li>
    </ul>
    <p class="sr" role="status">{t('mods.loading')}</p>
  {:else if ov && !ov.mods.length && !ov.problems.length}
    <div class="card empty">
      <div class="blk" aria-hidden="true"><span style="background:#FFD27A;transform:rotate(-6deg)"></span><span style="background:#B9D7F5;transform:translateY(-8px)"></span><span style="background:#BFE8CF;transform:rotate(5deg)"></span></div>
      <h2>{t('mods.emptyTitle')}</h2>
      <p class="muted">{t('mods.emptyText')}</p>
      <div class="row">
        <Button variant="primary" icon="search" onclick={() => mods.showBrowse()}>{t('mods.browseList')}</Button>
        <Button icon="link" onclick={() => mods.openFile()}>{t('mods.fromFileLong')}</Button>
      </div>
      <p class="hint"><Icon name="shield" size={14} /> {t('mods.reviewedHint')}</p>
    </div>
  {:else if ov && ov.mods.length}
    <div class="tools">
      <div class="srch" role="search">
        <Icon name="search" />
        <label class="sr" for="mods-q">{t('mods.search')}</label>
        <input id="mods-q" class="inp" type="search" placeholder={t('mods.search')} bind:value={mods.query} />
      </div>
      <div class="chips" role="group" aria-label={t('mods.filterAria')}>
        {#each chips as c (c.id)}
          <Chip pressed={mods.filter === c.id} icon={c.icon} count={c.count()} onclick={() => (mods.filter = c.id)}>{t(`mods.filter.${c.id}`)}</Chip>
        {/each}
        {#if mods.filter === 'client'}<Chip pressed icon="monitor" count={ov.clientMods.length} onclick={() => (mods.filter = 'all')}>{t('mods.filter.client')}</Chip>{/if}
      </div>
      {#if !viewport.phone}
        <label class="sr" for="mods-side">{t('mods.sidesLabel')}</label>
        <select id="mods-side" class="sel" bind:value={mods.side}>
          {#each sides as s (s)}<option value={`${s}`}>{t(`mods.sideFilter.${s}`)}</option>{/each}
        </select>
      {/if}
    </div>
    {#if list.length}
      <ul class="card rows" aria-label={t('mods.installedAria')}>
        {#each list as m (m.file)}<ModRow mod={m} compact={viewport.phone} />{/each}
      </ul>
    {:else}
      <div class="card none" data-testid="no-match">
        <Icon name="search" size={28} />
        <p class="strong">{mods.query.trim() ? t('mods.noMatch', { q: mods.query.trim() }) : t('mods.noneInFilter')}</p>
        <p class="hint">{t('mods.noMatchHint')}</p>
        <div class="row">
          <Button size="sm" onclick={() => { mods.query = ''; mods.filter = 'all'; mods.side = 'all'; }}>{t('mods.clearFilters')}</Button>
          <Button size="sm" variant="primary" onclick={() => mods.showBrowse(mods.query.trim())}>{t('mods.searchList')}</Button>
        </div>
      </div>
    {/if}
  {/if}
  {#if ov && !ov.error}
    <p class="hint foot">
      {t('mods.folder')} <span class="mono">{ov.dir}</span> · {fileSize(ov.size)}
      {#if desktop}<button class="link" onclick={() => mods.openFolder()}>{t('mods.openFolder')}</button>{/if}
    </p>
  {/if}
</div>

{#if viewport.phone}
  <button class="fab" onclick={() => mods.showBrowse()}><Icon name="plus" />{t('mods.add')}</button>
{/if}

<style>
  .main { padding: 20px 28px 32px; display: flex; flex-direction: column; gap: 16px; }
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); }
  .rows { list-style: none; margin: 0; padding: 4px 0; overflow: hidden; }
  .sk { display: flex; align-items: center; gap: 14px; padding: 12px 16px; min-height: 72px; }
  .col { display: flex; flex-direction: column; gap: 8px; }
  .ic { width: 44px; height: 44px; border-radius: 11px; }
  .l1 { width: 180px; height: 16px; }
  .l2 { width: 240px; height: 14px; }
  .l1.b { width: 140px; }
  .l2.b { width: 200px; }
  .l1.c { width: 210px; }
  .l2.c { width: 270px; }
  .skel {
    display: block;
    border-radius: 8px;
    background: linear-gradient(90deg, var(--surface-2), var(--surface-3), var(--surface-2));
    background-size: 200% 100%;
    animation: sk 1.4s ease-in-out infinite;
  }
  @keyframes sk { to { background-position: -200% 0; } }
  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 12px; padding: 44px 24px; }
  .empty h2 { font-size: 24px; }
  .empty p.muted { max-width: 560px; font-size: 15.5px; }
  .blk { display: flex; gap: 6px; margin-bottom: 6px; }
  .blk span { width: 44px; height: 44px; border-radius: 10px; box-shadow: 0 4px 0 rgba(0, 0, 0, 0.18); }
  .row { display: flex; gap: 10px; flex-wrap: wrap; justify-content: center; }
  .tools { display: flex; gap: 12px; flex-wrap: wrap; align-items: center; }
  .srch { position: relative; flex: 1 1 240px; min-width: 0; color: var(--text-3); }
  .srch :global(.ico) { position: absolute; left: 12px; top: 12px; pointer-events: none; }
  .inp {
    min-height: 44px;
    padding: 0 14px 0 42px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 15px var(--f-body);
    width: 100%;
    box-sizing: border-box;
  }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .sel {
    min-height: 40px;
    padding: 0 12px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 600 14px var(--f-body);
  }
  .none { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 10px; padding: 32px; color: var(--text-2); }
  .strong { font-weight: 700; color: var(--text); }
  .foot { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; overflow-wrap: anywhere; }
  .link { border: 0; background: none; color: var(--primary-ink); font: 700 13.5px var(--f-body); text-decoration: underline; cursor: pointer; padding: 0; }
  .path { font-size: 13px; overflow-wrap: anywhere; }
  .how { font-size: 14.5px; }
  .how summary { cursor: pointer; font-weight: 700; color: var(--primary-ink); }
  .how ul, .probs { margin: 8px 0 0; padding-left: 20px; display: flex; flex-direction: column; gap: 4px; }
  .probs li { overflow-wrap: anywhere; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .fab {
    position: fixed;
    right: 16px;
    bottom: calc(84px + env(safe-area-inset-bottom));
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 56px;
    padding: 0 20px;
    border-radius: 18px;
    background: var(--primary);
    color: var(--on-primary);
    font: 700 15px var(--f-body);
    box-shadow: 0 4px 0 var(--primary-edge), 0 8px 20px rgba(0, 0, 0, 0.2);
    border: 0;
    cursor: pointer;
    z-index: 11;
  }
  @media (max-width: 599px) {
    .main { padding: 6px 14px 96px; gap: 10px; }
  }
  @media (prefers-reduced-motion: reduce) { .skel { animation: none; } }
</style>
