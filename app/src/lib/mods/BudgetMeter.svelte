<script lang="ts">
  import { t } from '../i18n.svelte';

  /** `usedMs` against `budgetMs` on a scale to `maxMs`; over budget adds stripes, not only colour. */
  let { usedMs, budgetMs, maxMs, label }: { usedMs: number; budgetMs: number; maxMs: number; label: string } = $props();
  const pct = (v: number) => Math.min(100, Math.max(0, (v / maxMs) * 100));
  const fmt = (v: number) => v.toFixed(1);
</script>

<div
  class="meter"
  role="meter"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax={maxMs}
  aria-valuenow={usedMs}
  aria-valuetext={t('mods.meterText', { used: fmt(usedMs), budget: fmt(budgetMs) })}
>
  <span class:over={usedMs > budgetMs} style:width={`${pct(usedMs)}%`}></span>
  <i style:left={`calc(${pct(budgetMs)}% - 1px)`} title={t('mods.budget')}></i>
</div>

<style>
  .meter { position: relative; height: 14px; border-radius: 999px; background: var(--surface-3); border: 2px solid var(--line-strong); }
  span { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 999px; background: var(--mod-budget); }
  .over {
    background: repeating-linear-gradient(-45deg, var(--mod-budget-over) 0 6px, color-mix(in oklab, var(--mod-budget-over), #000 25%) 6px 12px);
  }
  i { position: absolute; top: -6px; bottom: -6px; width: 3px; border-radius: 2px; background: var(--text); }
</style>
