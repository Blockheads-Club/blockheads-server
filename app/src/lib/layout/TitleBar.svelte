<script lang="ts">
  import { onMount } from 'svelte';
  import { win } from '../api';
  import { t } from '../i18n.svelte';

  type Os = 'windows' | 'macos' | 'linux';
  interface Props {
    os: Os;
    /** The part after "Blockheads Server ·". */
    ctx: string;
    /** Running-server indicator; null while stopped. macOS draws its own buttons (overlay title bar). */
    run?: { label: string; tone: 'ok' | 'warn' | 'bad' } | null;
    /** Update state while the update popup is hidden (a button that opens it again); null: no update activity. */
    chip?: { label: string; tone: 'primary' | 'ok' | 'bad' | 'plain'; pct: number | null; onclick: () => void } | null;
  }
  let { os, ctx, run = null, chip = null }: Props = $props();

  let isMax = $state(false);
  let inactive = $state(false);

  onMount(() => {
    const sync = () => void win.isMaximized().then((m) => (isMax = m)).catch(() => {});
    sync();
    const offs: (() => void)[] = [];
    let gone = false;
    const keep = (p: Promise<() => void>) => void p.then((u) => (gone ? u() : offs.push(u))).catch(() => {});
    keep(win.onResized(sync));
    keep(win.onFocus((f) => (inactive = !f)));
    return () => {
      gone = true;
      offs.forEach((u) => u());
    };
  });

  const minimize = () => void win.minimize().catch(() => {});
  const toggle = () => void win.toggleMaximize().catch(() => {});
  const close = () => void win.close().catch(() => {});
  const maxLabel = $derived(isMax ? t('tb.restore') : t('tb.maximize'));
</script>

<header class={['bar', os, inactive && 'inactive']} aria-label={t('tb.label')} data-tauri-drag-region>
  {#if os === 'macos'}
    <div class="native" aria-hidden="true" data-tauri-drag-region></div>
  {/if}
  <div class="title" data-tauri-drag-region>
    <img src="/server-icon-small.svg" alt="" width="18" height="18" data-tauri-drag-region />
    <span class="name" data-tauri-drag-region>Blockheads <span class="tg">{t('app.server')}</span></span>
    <span class="ctx" data-tauri-drag-region>{`· ${ctx}`}</span>
    {#if chip}
      <button class={['chip', chip.tone]} aria-haspopup="dialog" aria-label={t('upd.chip.aria', { s: chip.label })} onclick={chip.onclick}>
        <svg class="ci" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 16.5v-9M8 11l4-4 4 4" /></svg>
        <span>{chip.label}</span>
        {#if chip.pct !== null}<span class="cb" style:width={`${chip.pct}%`}></span>{/if}
      </button>
    {/if}
  </div>
  {#if run}
    <span class={['run', run.tone]} role="status" data-tauri-drag-region><i aria-hidden="true"></i>{run.label}</span>
  {/if}
  {#if os === 'windows'}
    <div class="wctl" role="group" aria-label={t('tb.controls')}>
      <button class="w" aria-label={t('tb.minimize')} onclick={minimize}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button class="w" aria-label={maxLabel} onclick={toggle}>
        {#if isMax}
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".6" y="2.6" width="6.8" height="6.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M2.6 2.6V1.6A1 1 0 0 1 3.6.6h4.8a1 1 0 0 1 1 1v4.8a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        {:else}
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".6" y=".6" width="8.8" height="8.8" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        {/if}
      </button>
      <button class="w close" aria-label={t('tb.close')} onclick={close}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.8.8l8.4 8.4M9.2.8L.8 9.2" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  {:else if os === 'linux'}
    <div class="lctl" role="group" aria-label={t('tb.controls')}>
      <button class="l" aria-label={t('tb.minimize')} onclick={minimize}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 7h8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
      <button class="l" aria-label={maxLabel} onclick={toggle}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1.3" y="1.3" width="7.4" height="7.4" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
      </button>
      <button class="l close" aria-label={t('tb.close')} onclick={close}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 2l6 6M8 2L2 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
    </div>
  {/if}
</header>

<style>
  .bar {
    --b-bg: var(--surface-2);
    --b-line: var(--line);
    --b-fg: var(--text);
    --b-fg2: var(--text-3);
    --b-hov: rgba(31, 26, 23, 0.08);
    --b-round: var(--surface-3);
    --b-round-hov: #dcd2c6;
    box-sizing: border-box;
    flex: none;
    width: 100%;
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    background: var(--b-bg);
    border-bottom: 1px solid var(--b-line);
    color: var(--b-fg);
    user-select: none;
    -webkit-user-select: none;
  }
  :global(:root[data-theme='dark']) .bar { --b-bg: #1d1815; --b-line: #3a312b; --b-fg: #f5ede5; --b-fg2: #ae9f92; --b-hov: rgba(255,255,255,.12); --b-round: #312924; --b-round-hov: #4a413c; }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme='light']):not([data-theme='dark'])) .bar { --b-bg: #1d1815; --b-line: #3a312b; --b-fg: #f5ede5; --b-fg2: #ae9f92; --b-hov: rgba(255,255,255,.12); --b-round: #312924; --b-round-hov: #4a413c; }
  }
  .bar.inactive { --b-fg: var(--b-fg2); }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    height: 100%;
  }
  .macos .title { justify-content: center; padding-right: 72px; }
  .title img { flex: none; pointer-events: none; }
  .inactive .run { opacity: 0.6; }
  .inactive .title img { opacity: 0.55; }
  .tg { display: inline-block; margin-left: 2px; padding: 2px 6px 3px; border-radius: 6px; transform: rotate(-4deg); font-size: 12px; background: #56c990; color: #0e2a1c; box-shadow: 0 2px 0 #1e7a4c; }
  .run { display: inline-flex; align-items: center; gap: 6px; flex: none; margin: 0 10px; padding: 3px 10px; border-radius: 999px; font: 700 12px/1.4 var(--f-body); background: var(--ok-soft); color: var(--ok); }
  .run i { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  .run.warn { background: var(--warn-soft); color: var(--warn); }
  .run.bad { background: var(--bad-soft); color: var(--bad); }
  .name { font: 700 13.5px/1 var(--f-body); white-space: nowrap; color: var(--b-fg); }
  .ctx { font: 400 13px/1 var(--f-body); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--b-fg2); }
  button { border: 0; margin: 0; padding: 0; display: grid; place-items: center; cursor: default; font: inherit; color: var(--b-fg); background: transparent; }
  button:focus-visible { outline: 3px solid var(--focus); outline-offset: -3px; }
  .wctl { display: flex; height: 100%; flex: none; }
  .w { width: 46px; height: 100%; }
  .w:hover { background: var(--b-hov); }
  .w.close:hover { background: #c8301e; color: #fff; }
  .lctl { display: flex; gap: 10px; align-items: center; padding: 0 10px; flex: none; }
  .l { width: 28px; height: 28px; border-radius: 50%; background: var(--b-round); }
  .l:hover { background: var(--b-round-hov); }
  .l.close:hover { background: #c8301e; color: #fff; }
  .chip { position: relative; overflow: hidden; display: inline-flex; gap: 6px; align-items: center; flex: none; height: 24px; padding: 0 10px; border-radius: 999px; border: 1px solid var(--b-line); background: var(--surface); font: 700 12px/1 var(--f-body); cursor: pointer; }
  .chip:focus-visible { outline-offset: 1px; }
  .chip.primary { color: var(--primary-ink); }
  .chip.ok { color: var(--ok); }
  .chip.bad { color: var(--bad); }
  .chip.plain { color: var(--text-2); }
  .ci { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 2.2; stroke-linecap: round; stroke-linejoin: round; }
  .cb { position: absolute; left: 0; bottom: 0; height: 3px; background: var(--primary); }
  .native { width: 78px; height: 100%; flex: none; }
</style>
