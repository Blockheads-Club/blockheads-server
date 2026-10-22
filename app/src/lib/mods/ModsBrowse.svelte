<script lang="ts">
  import { ago, fileSize } from '../format';
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { BrowseMod } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { viewport } from '../viewport.svelte';
  import Chip from './Chip.svelte';
  import ModIcon from './ModIcon.svelte';
  import { mods, modErr, sidesOf, type BrowseSide, type BrowseSort } from './mods.svelte';
  import Notice from './Notice.svelte';
  import PermissionChip from './PermissionChip.svelte';
  import SidesBadge from './SidesBadge.svelte';

  const b = $derived(mods.browse);
  const list = $derived(mods.browseList);
  const sideOpts: BrowseSide[] = ['any', 'server', 'client'];
  const sorts: BrowseSort[] = ['updated', 'newest', 'name'];
  const installedIds = $derived(new Set(mods.mods.map((m) => m.id)));
  const updated = (m: BrowseMod) => (m.published ? t('browse.updated', { ago: ago(Math.max(0, store.now - Date.parse(m.published))) }) : '');
  function clear() {
    mods.bquery = '';
    mods.bside = 'any';
    mods.hideNative = false;
    mods.tag = '';
  }
</script>

<PageHeader title={t('browse.title')} subtitle={t('browse.subtitle')}>
  {#snippet actions()}
    <Button icon="link" size={viewport.phone ? 'sm' : 'md'} onclick={() => mods.openFile()}>{t('mods.fromFile')}</Button>
  {/snippet}
</PageHeader>

<div class="main">
  <button class="back" onclick={() => mods.showList()}><Icon name="chev-l" size={18} />{t('nav.mods')}</button>
  <div class="tools">
    <div class="srch" role="search">
      <Icon name="search" />
      <label class="sr" for="browse-q">{t('browse.search')}</label>
      <input id="browse-q" class="inp" type="search" placeholder={t('browse.search')} bind:value={mods.bquery} />
    </div>
    <div class="seg" role="group" aria-label={t('browse.sidesAria')}>
      {#each sideOpts as s (s)}
        <button aria-pressed={mods.bside === s} onclick={() => (mods.bside = s)}>
          {#if s !== 'any'}<Icon name={s === 'server' ? 'server' : 'monitor'} size={16} />{/if}{t(`browse.side.${s}`)}
        </button>
      {/each}
    </div>
    <Toggle bind:checked={mods.hideNative} label={t('browse.hideNative')} />
    <label class="sr" for="browse-sort">{t('browse.sort')}</label>
    <select id="browse-sort" class="sel" bind:value={mods.sort}>
      {#each sorts as s (s)}<option value={`${s}`}>{t(`browse.sort.${s}`)}</option>{/each}
    </select>
  </div>
  {#if mods.tags.length}
    <div class="chips" role="group" aria-label={t('browse.tagsAria')}>
      <Chip pressed={mods.tag === ''} onclick={() => (mods.tag = '')}>{t('browse.allTags')}</Chip>
      {#each mods.tags as tag (tag)}<Chip pressed={mods.tag === tag} onclick={() => (mods.tag = tag)}>{tag}</Chip>{/each}
    </div>
  {/if}

  {#if mods.browseLoading && !b}
    <div class="grid" aria-busy="true">
      {#each [1, 2, 3, 4] as i (i)}<div class="card sk"><span class="skel" style="width:44px;height:44px"></span><span class="skel" style="width:70%;height:16px"></span><span class="skel" style="width:90%;height:14px"></span></div>{/each}
    </div>
    <p class="sr" role="status">{t('browse.loading')}</p>
  {:else if mods.browseError}
    <Notice tone="bad" icon="wifi" title={t('browse.offline')}>
      <p>{t('browse.offlineText')}</p>
      <p class="mono small">{modErr(mods.browseError)}</p>
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="refresh" onclick={() => mods.loadBrowse()}>{t('mods.tryAgain')}</Button>
        <Button size="sm" icon="file" onclick={() => mods.openFile()}>{t('browse.fromFile')}</Button>
      {/snippet}
    </Notice>
  {:else if b}
    {#if b.offline}
      <Notice tone="warn" icon="wifi" title={t('browse.cached')}>
        <p>{t('browse.cachedText', { date: b.generated.slice(0, 10) })}</p>
        {#snippet actions()}<Button size="sm" icon="refresh" onclick={() => mods.loadBrowse()}>{t('mods.tryAgain')}</Button>{/snippet}
      </Notice>
    {/if}
    {#if !b.mods.length}
      <div class="card empty"><Icon name="puzzle" size={32} /><h2>{t('browse.emptyTitle')}</h2><p class="muted">{t('browse.emptyText')}</p></div>
    {:else if !list.length}
      <div class="card empty" data-testid="browse-none">
        <Icon name="search" size={32} />
        <h2>{mods.bquery.trim() ? t('browse.none', { q: mods.bquery.trim() }) : t('browse.noneFilters')}</h2>
        <p class="muted">{t('browse.noneText')}</p>
        <Button size="sm" onclick={clear}>{t('mods.clearFilters')}</Button>
      </div>
    {:else}
      <p class="hint" role="status">{t('browse.count', { n: list.length, total: b.mods.length })}</p>
      <div class="grid">
        {#each list as m (m.id)}
          {@const deps = Object.keys(m.depends)}
          <article class={['card', 'mod', m.native && 'danger']} data-testid="browse-card">
            {#if m.native}<div class="dz-h"><Icon name="octagon" /><span>{t('browse.nativeBand')}</span></div>{/if}
            <div class="body">
              <div class="top">
                <ModIcon id={m.id} name={m.name} size={viewport.phone ? 'sm' : 'md'} />
                <div class="grow">
                  <h3>{m.name}</h3>
                  <p class="hint">{t('mods.by', { name: m.owners.join(', ') })}{#if m.tags[0]}{` · ${m.tags[0]}`}{/if}</p>
                </div>
              </div>
              {#if m.description}<p class="muted desc">{m.description}</p>{/if}
              <div class="badges">
                <SidesBadge sides={sidesOf(m.sides.includes('server'), m.clientRequired || m.sides.includes('client'))} />
                {#each m.permissions as p (p)}<PermissionChip perm={p} />{/each}
              </div>
              {#if deps.length}<p class="hint"><Icon name="link" size={14} /> {t('browse.needs', { names: deps.map((d) => (installedIds.has(d) ? d : t('browse.addedForYou', { name: d }))).join(', ') })}</p>{/if}
              {#if m.clientRequired}<p class="hint"><Icon name="monitor" size={14} />{` ${t('browse.loader')}`}</p>{/if}
              <div class="foot">
                <span class="hint meta"><span class="rev"><Icon name="shield" size={14} />{t('mods.reviewed')}</span>{#if m.size}<span>{fileSize(m.size)}</span>{/if}<span>{updated(m)}</span></span>
                {#if m.installed === m.version}
                  <Badge tone="ok" icon="check">{t('browse.installed')}</Badge>
                {:else if m.installed}
                  <Button size="sm" icon="update" onclick={() => mods.install(m.id)}>{t('browse.update')}</Button>
                {:else}
                  <Button size="sm" variant={m.native ? 'danger' : 'primary'} icon="download" label={t('browse.installAria', { name: m.name })} onclick={() => mods.install(m.id)}>{t('mods.install')}</Button>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .main { padding: 16px 28px 32px; display: flex; flex-direction: column; gap: 14px; }
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
  .seg { display: inline-flex; border: 2px solid var(--line-strong); border-radius: var(--r-md); overflow: hidden; flex: none; }
  .seg button {
    min-height: 36px;
    padding: 0 12px;
    border: 0;
    background: var(--surface);
    color: var(--text-2);
    font: 700 13.5px var(--f-body);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .seg button + button { border-left: 2px solid var(--line-strong); }
  .seg button[aria-pressed='true'] { background: var(--text); color: var(--bg); }
  .sel {
    min-height: 40px;
    padding: 0 12px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 600 14px var(--f-body);
  }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr)); gap: 14px; }
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); min-width: 0; }
  .sk { padding: 16px; display: flex; flex-direction: column; gap: 10px; }
  .skel {
    display: block;
    border-radius: 8px;
    background: linear-gradient(90deg, var(--surface-2), var(--surface-3), var(--surface-2));
    background-size: 200% 100%;
    animation: sk 1.4s ease-in-out infinite;
  }
  @keyframes sk { to { background-position: -200% 0; } }
  .mod { display: flex; flex-direction: column; overflow: hidden; }
  .mod.danger { border-color: var(--mod-danger-edge); background: var(--bad-soft); }
  .dz-h {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    color: var(--mod-on-danger);
    font: 800 13px var(--f-body);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    background: repeating-linear-gradient(-45deg, var(--mod-danger) 0 12px, var(--mod-danger-stripe) 12px 24px);
  }
  .body { padding: 16px; display: flex; flex-direction: column; gap: 10px; flex: 1; }
  .top { display: flex; gap: 12px; align-items: center; }
  .grow { flex: 1; min-width: 0; }
  h3 { font-size: 17px; overflow-wrap: anywhere; }
  .desc { font-size: 14.5px; }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; }
  .foot { margin-top: auto; display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; }
  .meta { display: inline-flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .rev { display: inline-flex; align-items: center; gap: 5px; font: 700 12.5px var(--f-body); color: var(--ok); }
  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 10px; padding: 36px 24px; color: var(--text-2); }
  .empty h2 { font-size: 20px; color: var(--text); }
  .small { font-size: 12.5px; overflow-wrap: anywhere; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  @media (max-width: 599px) {
    .main { padding: 6px 14px 20px; gap: 10px; }
  }
  @media (prefers-reduced-motion: reduce) { .skel { animation: none; } }
</style>
