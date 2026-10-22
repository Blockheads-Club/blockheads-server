<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from '../ui/Icon.svelte';

  let { pressed, icon, count, onclick, children }: { pressed: boolean; icon?: IconName; count?: number; onclick: () => void; children: Snippet } =
    $props();
</script>

<button class="chip" aria-pressed={pressed} {onclick}>
  {#if icon}<Icon name={icon} size={16} />{/if}{@render children()}{#if count !== undefined}<span class="n">{count}</span>{/if}
</button>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 12px;
    border-radius: 999px;
    border: 2px solid var(--line);
    background: var(--surface);
    color: var(--text-2);
    font: 700 13.5px var(--f-body);
    cursor: pointer;
    white-space: nowrap;
  }
  .chip[aria-pressed='true'] { background: var(--text); border-color: var(--text); color: var(--bg); }
  .n { font: 600 12px var(--f-mono); opacity: 0.8; }
</style>
