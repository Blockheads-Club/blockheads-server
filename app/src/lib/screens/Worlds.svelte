<script lang="ts">
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { NewWorld, World, WorldSize } from '../types';
  import Alert from '../ui/Alert.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import EmptyState from '../ui/EmptyState.svelte';
  import Field from '../ui/Field.svelte';
  import Icon from '../ui/Icon.svelte';
  import Menu from '../ui/Menu.svelte';
  import Table from '../ui/Table.svelte';
  import Tabs from '../ui/Tabs.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { copyText, avatarColor, shortId } from '../util';
  import { toast } from '../ui/toast.svelte';
  import { viewport } from '../viewport.svelte';

  const sizes: WorldSize[] = ['tiny', 'small', 'normal', 'large', 'huge'];
  let query = $state('');
  let creating = $state(false);
  let busy = $state(false);
  let form = $state<NewWorld>({ name: '', size: 'normal', expert: false });
  let loadNow = $state(true);
  let removing = $state<World | null>(null);

  const q = $derived(query.trim().toLowerCase());
  const list = $derived(store.worlds.filter((w) => w.name.toLowerCase().includes(q)));
  const loaded = $derived(store.active ? store.runningWorldId : null);

  function open() {
    form = { name: '', size: 'normal', expert: false };
    loadNow = true;
    creating = true;
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    const ok = await store.createWorld($state.snapshot(form), loadNow);
    busy = false;
    if (!ok) return;
    creating = false;
    toast(t('worlds.created', { name: form.name.trim() }));
  }

  async function confirmRemove() {
    const w = removing!;
    removing = null;
    await store.deleteWorld(w.id);
  }

  async function pick(w: World, id: string) {
    if (id === 'copy') toast((await copyText(w.id)) ? t('worlds.idCopied') : t('qr.copyFailed'));
    else removing = w;
  }
</script>

<PageHeader title={t('worlds.title')} subtitle={store.info?.platform === 'android' ? t('worlds.whereAndroid') : t('worlds.where')}>
  {#snippet actions()}
    {#if !viewport.phone}<Button icon="upload" disabled title={t('common.soon')}>{t('worlds.import')}</Button>{/if}
    <Button variant="primary" icon="plus" onclick={open}>{t('worlds.create')}</Button>
  {/snippet}
</PageHeader>

<div class="main">
  {#if store.worlds.length === 0}
    <div class="card">
      <EmptyState icon="map" title={t('worlds.emptyTitle')} text={t('worlds.empty')}>
        <Button variant="primary" icon="plus" onclick={open}>{t('worlds.create')}</Button>
        <Button icon="upload" disabled title={t('common.soon')}>{t('worlds.import')}</Button>
      </EmptyState>
    </div>
  {:else}
    <div class="search">
      <Field label={t('worlds.search')} hideLabel icon="search" type="search" placeholder={t('worlds.search')} bind:value={query} />
    </div>
    {#if list.length === 0}
      <p class="muted" data-testid="no-match">{t('worlds.noMatch', { q: query.trim() })} <button class="link" onclick={() => (query = '')}>{t('common.clearSearch')}</button></p>
    {:else}
      <Table
        label={t('worlds.title')}
        template="52px minmax(0, 2fr) minmax(0, 1fr) auto"
        columns={[
          { key: 'i', label: t('worlds.colIcon'), srOnly: true },
          { key: 'n', label: t('worlds.colName') },
          { key: 's', label: t('worlds.colStatus'), narrow: true },
          { key: 'a', label: t('worlds.colActions'), align: 'end' }
        ]}
      >
        {#snippet rows()}
          {#each list as w (w.id)}
            {@const isLoaded = loaded === w.id}
            {@const isSel = !isLoaded && store.settings.run.worldId === w.id}
            <div class={["tr", isLoaded && "hl"]} role="row" data-testid="world">
              <span class="wi" role="cell" style:background={avatarColor(w.name)}><Icon name="map" /></span>
              <span role="cell" class="nm">
                <span class="line">
                  <span class="wn">{w.name}</span>
                  {#if isLoaded}<Badge tone="ok" dot>{t('worlds.loaded')}</Badge>{:else if isSel}<Badge tone="primary">{t('worlds.selected')}</Badge>{/if}
                </span>
                <span class="hint mono">{t('worlds.id')} {shortId(w.id)}</span>
              </span>
              <span role="cell" class="narrow">
                {#if isLoaded}{t('worlds.loadedNow', { n: store.players.length })}{:else if isSel}{t('worlds.willStart')}{:else}—{/if}
              </span>
              <span role="cell" class="acts">
                {#if isLoaded}
                  <Button size="sm" disabled>{t('worlds.loadedBtn')}</Button>
                {:else}
                  <Button size="sm" icon="play" onclick={() => store.loadWorld(w.id)}>{t('worlds.load')}</Button>
                {/if}
                <Menu
                  label={t('worlds.moreFor', { name: w.name })}
                  items={[
                    { id: 'copy', label: t('worlds.copyId'), icon: 'copy' },
                    { id: 'delete', label: t('worlds.deleteAsk'), icon: 'trash', tone: 'bad', separated: true }
                  ]}
                  onselect={(id) => pick(w, id)}
                />
              </span>
            </div>
          {/each}
        {/snippet}
      </Table>
      <p class="hint">{t('worlds.restartNote')}</p>
    {/if}
  {/if}
</div>

{#if creating}
  <form onsubmit={create}>
    <Dialog title={t('worlds.createTitle')} onclose={() => (creating = false)}>
      {#if store.error}<Alert tone="bad" title={store.error} />{/if}
      <Field label={t('worlds.name')} bind:value={form.name} maxlength={30} required hint={t('worlds.nameHint')} />
      <div>
        <p class="lbl" id="wsz">{t('worlds.size')}</p>
        <Tabs mode="radiogroup" label={t('worlds.size')} items={sizes.map((s) => ({ id: s, label: t(`size.${s}`) }))} value={form.size} onchange={(s) => (form.size = s)} />
        <p class="hint gap">{t('worlds.sizeHint')}</p>
      </div>
      <div class="expert">
        <div>
          <p class="strong">{t('worlds.expert')}</p>
          <p class="hint">{t('worlds.expertHint')}</p>
        </div>
        <Toggle bind:checked={form.expert} label={t('worlds.expert')} />
      </div>
      <details class="rules">
        <summary>{t('worlds.rules')}</summary>
        <p class="hint">{t('worlds.rulesSoon')}</p>
      </details>
      <label class="chk"><input type="checkbox" bind:checked={loadNow} />{store.active ? t('worlds.loadNowRestart') : t('worlds.loadNow')}</label>
      {#snippet footer()}
        <Button onclick={() => (creating = false)}>{t('common.cancel')}</Button>
        <Button variant="primary" icon="plus" type="submit" loading={busy} disabled={!form.name.trim()}>{t('worlds.createBtn')}</Button>
      {/snippet}
    </Dialog>
  </form>
{/if}

{#if removing}
  <Dialog title={t('worlds.deleteTitle', { name: removing.name })} icon="trash" tone="bad" description={t('worlds.deleteText')} onclose={() => (removing = null)}>
    {#if store.active}<p class="hint warn">{t('err.running')}</p>{/if}
    {#snippet footer()}
      <Button onclick={() => (removing = null)}>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" disabled={store.active} onclick={confirmRemove}>{t('worlds.deleteBtn')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .main { padding: 24px 28px; display: flex; flex-direction: column; gap: 16px; }
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); }
  .search { max-width: 360px; }
  .wi {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    transform: rotate(-4deg);
    box-shadow: 0 2px 0 rgba(0, 0, 0, 0.18);
    color: #1f1a17;
  }
  .hl { background: color-mix(in oklab, var(--primary-soft), transparent 40%); }
  .nm { min-width: 0; display: flex; flex-direction: column; }
  .line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .wn { font-weight: 700; font-size: 16.5px; overflow-wrap: anywhere; }
  .acts { display: flex; gap: 6px; justify-content: flex-end; align-items: center; }
  .link { border: 0; background: none; color: var(--primary-ink); font: 700 14.5px var(--f-body); text-decoration: underline; cursor: pointer; }
  .lbl { font-weight: 700; font-size: 14.5px; margin-bottom: 6px; }
  .gap { margin-top: 6px; }
  .strong { font-weight: 700; }
  .expert { display: flex; justify-content: space-between; align-items: center; gap: 14px; padding: 12px 14px; border-radius: 12px; background: var(--surface-2); }
  .rules { border: 2px solid var(--line); border-radius: 12px; padding: 4px 14px 12px; }
  .rules summary { cursor: pointer; font-weight: 700; min-height: 44px; display: flex; align-items: center; }
  .chk { display: flex; gap: 10px; align-items: center; font-size: 15px; min-height: 44px; }
  .chk input { width: 20px; height: 20px; accent-color: var(--primary); margin: 0; }
  .warn { color: var(--bad); font-weight: 700; }
  @media (max-width: 599px) {
    .main { padding: 6px 16px 20px; }
  }
</style>
