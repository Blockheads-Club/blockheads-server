<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Badge from '../ui/Badge.svelte';
  import Icon from '../ui/Icon.svelte';
  import StatusPill from '../ui/StatusPill.svelte';
  import { NAV } from './nav';

  /** `rail` is the 72px tablet variant: icons with labels below, no status card. */
  let { rail = false }: { rail?: boolean } = $props();
  const meta = $derived(`v${store.info?.version ?? '-'} · ${t('side.port', { port: store.settings.run.port })}`);
</script>

<nav class={["side", rail && "rail"]} aria-label={t('nav.aria')}>
  <div class="brand">
    <img src="/logo.svg" alt="" width="40" height="40" />
    {#if !rail}<span><small>Blockheads Club</small><b>{t('app.server')}</b></span>{/if}
  </div>
  {#each NAV as it (it.id)}
    <button class="nv" aria-current={store.screen === it.id ? 'page' : undefined} onclick={() => store.navigate(it.id)}>
      <Icon name={it.icon} />
      <span class="lbl">{t(`nav.${it.id}`)}</span>
      {#if it.id === 'players' && store.players.length > 0}
        <span class="cnt"><Badge label={t('players.onlineCount', { n: store.players.length })}>{store.players.length}</Badge></span>
      {/if}
    </button>
  {/each}
  {#if !rail}
    <div class="sstat">
      <StatusPill state={store.state} />
      {#if store.currentWorld}<p class="world">{store.currentWorld.name}</p>{/if}
      <p class="hint mono">{meta}</p>
    </div>
  {/if}
</nav>

<style>
  .side {
    background: var(--surface);
    border-right: 2px solid var(--line);
    padding: 16px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 240px;
    flex: none;
    position: sticky;
    top: 0;
    height: 100vh;
    overflow: auto;
  }
  .side.rail { width: 84px; padding: 12px 6px; align-items: stretch; }
  .brand { display: flex; align-items: center; gap: 10px; padding: 4px 8px 14px; }
  .rail .brand { justify-content: center; padding: 4px 0 12px; }
  .brand img { border-radius: 10px; }
  small { display: block; font: 700 12px var(--f-body); color: var(--text-3); }
  b { display: block; font: 800 19px/1.1 var(--f-display); }
  .nv {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 12px;
    border-radius: var(--r-md);
    border: 0;
    background: transparent;
    color: var(--text-2);
    font: 700 16px var(--f-body);
    text-align: left;
    cursor: pointer;
  }
  .nv:hover { background: var(--surface-2); color: var(--text); }
  .nv[aria-current='page'] { background: var(--primary-soft); color: var(--primary-ink); }
  .cnt { margin-left: auto; }
  .rail .nv { flex-direction: column; gap: 2px; padding: 8px 2px; min-height: 56px; font-size: 11.5px; text-align: center; position: relative; }
  .rail .cnt { position: absolute; top: 2px; right: 4px; }
  .sstat {
    margin-top: auto;
    padding: 12px;
    border-radius: 12px;
    background: var(--surface-2);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sstat :global(span[role='status']) { align-self: flex-start; }
  .world { font-weight: 700; font-size: 14.5px; overflow-wrap: anywhere; }
  .hint.mono { font-size: 12.5px; }
</style>
