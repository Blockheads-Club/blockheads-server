<script lang="ts">
  import { bytes, percent } from '../format';
  import { t } from '../i18n.svelte';
  import type { Stats } from '../types';

  /** CPU and memory the game server uses, with the last minute as small graphs. Nothing while it is off. */
  let { stats, history, running }: { stats: Stats | null; history: Stats[]; running: boolean } = $props();

  /** Polyline points for `values` (0..max) in a 100 x 28 box, newest at the right. */
  function line(values: number[], max: number): string {
    if (values.length < 2) return '';
    const step = 100 / (values.length - 1);
    return values.map((v, i) => `${(i * step).toFixed(1)},${(28 - (Math.min(v, max) / max) * 26 - 1).toFixed(1)}`).join(' ');
  }
  const cpuLine = $derived(line(history.map((s) => s.cpu), 100));
  const memMax = $derived(Math.max(1, ...history.map((s) => s.mem)) * 1.2);
  const memLine = $derived(line(history.map((s) => s.mem), memMax));
</script>

<div class="perf" data-testid="perf">
  <div class="ph"><h2>{t('perf.title')}</h2><p class="hint">{t('perf.hint')}</p></div>
  {#if !running}
    <p class="muted">{t('perf.off')}</p>
  {:else if stats}
    <div class="grid">
      <div class="m">
        <p class="k">{t('perf.cpu')}</p>
        <p class="v"><span data-testid="perf-cpu">{percent(stats.cpu)}</span> <span class="hint">{t('perf.cpuOf')}</span></p>
        <div class="meter" aria-hidden="true"><span style:width={`${Math.min(100, stats.cpu)}%`}></span></div>
        <svg class="spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden="true"><polyline points={cpuLine} /></svg>
      </div>
      <div class="m">
        <p class="k">{t('perf.ram')}</p>
        <p class="v"><span data-testid="perf-mem">{bytes(stats.mem)}</span></p>
        <svg class="spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden="true"><polyline points={memLine} /></svg>
      </div>
    </div>
    {#if !stats.threadCpu}<p class="hint">{t('perf.appCpu')}</p>{/if}
  {:else}
    <p class="muted">{t('perf.waiting')}</p>
  {/if}
</div>

<style>
  .perf { display: flex; flex-direction: column; gap: 12px; }
  .ph { display: flex; justify-content: space-between; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  h2 { font-size: 20px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(240px, 100%), 1fr)); gap: 16px 24px; }
  .m { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .k { font-size: 13.5px; font-weight: 700; color: var(--text-3); }
  .v { font: 800 24px/1.1 var(--f-display); }
  .v .hint { font: 400 13.5px var(--f-body); }
  .meter { height: 8px; border-radius: 999px; background: var(--surface-3); overflow: hidden; }
  .meter span { display: block; height: 100%; border-radius: 999px; background: var(--primary); transition: width var(--dur-move, 0.2s); }
  .spark { width: 100%; height: 36px; margin-top: auto; }
  .spark polyline { fill: none; stroke: var(--primary); stroke-width: 1.6; vector-effect: non-scaling-stroke; stroke-linejoin: round; }
</style>
