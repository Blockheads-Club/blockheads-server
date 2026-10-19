<script lang="ts">
  import type { Snippet } from 'svelte';
  import { store } from '../store.svelte';
  import StatusPill from '../ui/StatusPill.svelte';
  import { viewport } from '../viewport.svelte';

  let { title, subtitle, actions, compact = false }: { title: string; subtitle?: string; actions?: Snippet; compact?: boolean } = $props();
</script>

<header class="head" class:compact class:phone={viewport.phone}>
  {#if viewport.phone}<img src="/server-icon.svg" alt="" width="34" height="34" />{/if}
  <div class="titles">
    <h1>{title}</h1>
    {#if subtitle && !viewport.phone}<p class="muted">{subtitle}</p>{/if}
  </div>
  {#if viewport.phone}<StatusPill state={store.state} />{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 20px 28px;
    border-bottom: 2px solid var(--line);
    flex-wrap: wrap;
  }
  .compact { padding: 16px 24px; }
  h1 { font-size: 28px; }
  .compact h1 { font-size: 26px; }
  .titles { flex: 1 1 auto; min-width: 0; }
  .titles p { font-size: 15px; }
  .actions { display: flex; gap: 10px; flex-wrap: wrap; }
  .phone { padding: 10px 16px; border-bottom: 0; flex-wrap: nowrap; gap: 10px; }
  .phone img { border-radius: 8px; }
  .phone h1 { font-size: 20px; font-weight: 800; }
  .phone .actions { flex-basis: 100%; }
  .phone { flex-wrap: wrap; }
</style>
