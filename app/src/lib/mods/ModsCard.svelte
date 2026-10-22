<script lang="ts">
  import { t } from '../i18n.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import { mods } from './mods.svelte';

  /** Dashboard: how the mods are doing, with the way to manage them. */
  const ov = $derived(mods.overview);
  const failed = $derived(mods.mods.filter((m) => m.status === 'failed' || m.status === 'budget').length);
  const line = $derived.by(() => {
    if (!ov || !ov.mods.length) return t('mods.cardEmpty');
    const s = t('mods.cardRunning', { n: mods.running.length, total: ov.mods.length });
    return ov.clientMods.length ? `${s} · ${t('mods.cardLoader')}` : s;
  });
</script>

<section class="card" aria-labelledby="mdc">
  <span class="ic"><Icon name="puzzle" /></span>
  <div class="txt">
    <h2 id="mdc">{t('nav.mods')}</h2>
    <p class="muted">{line}</p>
  </div>
  <div class="badges">
    {#if failed}<Badge tone="bad" icon="x-circle">{t('mods.cardFailed', { n: failed })}</Badge>{/if}
    {#if mods.waiting.length}<Badge tone="warn">{t('mods.cardWaiting', { n: mods.waiting.length })}</Badge>{/if}
    {#if mods.updates.length}<Badge tone="primary">{t('mods.cardUpdates', { n: mods.updates.length })}</Badge>{/if}
  </div>
  {#if ov && ov.mods.length}
    <Button size="sm" onclick={() => mods.showList()}>{t('mods.manage')}</Button>
  {:else}
    <Button size="sm" icon="search" onclick={() => mods.showBrowse()}>{t('mods.browse')}</Button>
  {/if}
</section>

<style>
  .card {
    background: var(--surface);
    border: 2px solid var(--line);
    border-radius: var(--r-lg);
    padding: 16px 20px;
    display: flex;
    gap: 16px;
    align-items: center;
    flex-wrap: wrap;
  }
  .ic { width: 44px; height: 44px; border-radius: 12px; background: var(--primary-soft); color: var(--primary-ink); display: grid; place-items: center; flex: none; }
  .txt { flex: 1 1 260px; min-width: 0; }
  h2 { font-size: 19px; }
  .txt p { font-size: 14.5px; }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
