<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { ListKind } from '../types';
  import Alert from '../ui/Alert.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Card from '../ui/Card.svelte';
  import ChipInput from '../ui/ChipInput.svelte';
  import EmptyState from '../ui/EmptyState.svelte';
  import Icon from '../ui/Icon.svelte';
  import { toast } from '../ui/toast.svelte';
  import { viewport } from '../viewport.svelte';

  const kinds: { kind: ListKind; icon: IconName; tint: string; add: IconName }[] = [
    { kind: 'admin', icon: 'crown', tint: 'sun', add: 'plus' },
    { kind: 'mod', icon: 'shield', tint: 'info', add: 'plus' },
    { kind: 'white', icon: 'check', tint: 'ok', add: 'plus' },
    { kind: 'black', icon: 'ban', tint: 'bad', add: 'ban' }
  ];

  async function add(kind: ListKind, name: string) {
    if (await store.addName(kind, name)) toast(t('access.added', { name, list: t(`access.${kind}`) }), { label: t('common.undo'), run: () => store.removeName(kind, name) });
  }
</script>

<PageHeader title={t('nav.access')} subtitle={t('access.subtitle')}>
  {#snippet actions()}
    {#if !viewport.phone}<Button icon="upload" disabled title={t('common.soon')}>{t('access.import')}</Button>{/if}
  {/snippet}
</PageHeader>

<div class="main">
  {#if !store.settings.run.worldId}
    <div class="wide"><Card><EmptyState icon="map" title={t('access.noWorldTitle')} text={t('run.noWorld')}>
      <Button variant="primary" icon="map" onclick={() => store.navigate('worlds')}>{t('nav.worlds')}</Button>
    </EmptyState></Card></div>
  {:else}
    {#if store.active}
      <div class="wide"><Alert tone="info" title={t('lists.liveNote')} /></div>
    {/if}
    {#each kinds as k (k.kind)}
      <Card class="lst" label={t(`access.${k.kind}`)}>
        <div class="lh">
          <span class={['li', k.tint]}><Icon name={k.icon} /></span>
          <div class="lt">
            <h2>{t(`access.${k.kind}`)} <Badge>{store.lists[k.kind].length}</Badge></h2>
            <p class="hint">{t(`access.${k.kind}Hint`)}</p>
          </div>
        </div>
        <ChipInput
          label={t(`access.addTo.${k.kind}`)}
          placeholder={t(`access.ph.${k.kind}`)}
          addLabel={t(`access.addBtn.${k.kind}`)}
          addIcon={k.add}
          entries={store.lists[k.kind]}
          removeLabel={(n) => t(`access.remove.${k.kind}`, { name: n })}
          removeText={k.kind === 'black' ? t('access.unban') : undefined}
          emptyText={t('lists.empty')}
          onadd={(n) => add(k.kind, n)}
          onremove={(n) => store.removeName(k.kind, n)}
        />
      </Card>
    {/each}
  {/if}
</div>

<style>
  .main {
    padding: 24px 28px;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
    gap: 18px;
    align-content: start;
  }
  .wide { grid-column: 1 / -1; }
  .main :global(.lst) { padding: 18px; display: flex; flex-direction: column; gap: 12px; }
  .lh { display: flex; gap: 12px; align-items: center; }
  .lt { flex: 1; min-width: 0; }
  h2 { font-size: 20px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .li {
    width: 40px;
    height: 40px;
    border-radius: 11px;
    display: grid;
    place-items: center;
    flex: none;
    transform: rotate(-6deg);
    box-shadow: 0 2px 0 rgba(0, 0, 0, 0.18);
  }
  .sun { background: var(--sun); color: #1f1a17; }
  .info { background: var(--info-soft); color: var(--info); }
  .ok { background: var(--ok-soft); color: var(--ok); }
  .bad { background: var(--bad-soft); color: var(--bad); }
  @media (max-width: 599px) {
    .main { padding: 6px 16px 20px; gap: 12px; }
  }
</style>
