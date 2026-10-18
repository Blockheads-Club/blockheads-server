<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { Screen } from '../types';
  import Card from '../ui/Card.svelte';
  import Icon from '../ui/Icon.svelte';

  /** Phone only: what does not fit in the bottom bar. */
  const items: { id: Screen; icon: IconName }[] = [
    { id: 'access', icon: 'shield' },
    { id: 'settings', icon: 'sliders' }
  ];
</script>

<PageHeader title={t('nav.more')} />
<div class="main">
  <Card pad={false}>
    {#each items as it (it.id)}
      <button class="item" onclick={() => store.navigate(it.id)}>
        <Icon name={it.icon} /><span class="grow">{t(`nav.${it.id}`)}</span><Icon name="chev-r" />
      </button>
    {/each}
  </Card>
</div>

<style>
  .main { padding: 6px 16px 20px; }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 56px;
    padding: 0 16px;
    border: 0;
    border-top: 1px solid var(--line);
    background: transparent;
    color: var(--text);
    font: 700 16px var(--f-body);
    text-align: left;
    cursor: pointer;
  }
  .item:first-child { border-top: 0; }
  .grow { flex: 1; }
</style>
