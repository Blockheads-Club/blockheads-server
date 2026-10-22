<script lang="ts">
  import { percent } from '../format';
  import { t } from '../i18n.svelte';
  import Icon from '../ui/Icon.svelte';
  import { updates } from '../update.svelte';

  /**
   * The update icon (design: Update-Parts). Always there, so "Check for updates" is one click away; its dot and
   * label show the state, a click opens the popup. `row`: bottom of the side menu; `icon`: the tablet rail;
   * `item`: a row of the phone's More list. Colour is never the only signal: every state has a label.
   */
  let { variant = 'row' }: { variant?: 'row' | 'icon' | 'item' } = $props();
  const u = updates;
  const pct = $derived(percent(u.percent));

  const view = $derived.by(() => {
    const v = u.info?.version ?? '';
    switch (u.stage) {
      case 'checking':
        return { tone: '', text: t('upd.checking'), aria: t('upd.checking'), pip: '' };
      case 'available':
        return { tone: 'primary', text: t('upd.icon.available'), aria: t('upd.aria.available', { v }), pip: 'primary' };
      case 'downloading':
        return { tone: 'primary', text: t('upd.icon.downloading', { p: pct }), aria: t('upd.aria.downloading', { p: pct }), pip: '' };
      case 'ready':
        return { tone: 'ok', text: t('upd.icon.ready'), aria: t('upd.aria.ready'), pip: 'ok' };
      case 'installing':
        return { tone: '', text: t('upd.icon.installing'), aria: t('upd.icon.installing'), pip: '' };
      case 'error':
        return u.failed === 'check'
          ? { tone: '', text: t('upd.check'), aria: t('upd.aria.checkFailed'), pip: 'bad' }
          : { tone: 'bad', text: t('upd.icon.failed'), aria: t('upd.aria.failed'), pip: 'bad' };
      default:
        return { tone: '', text: t('upd.check'), aria: t('upd.check'), pip: '' };
    }
  });
</script>

<button
  class={['upd', variant, view.tone]}
  aria-haspopup="dialog"
  aria-label={view.aria}
  title={variant === 'icon' ? view.aria : undefined}
  data-testid="update-button"
  onclick={() => u.openPopup()}
>
  <span class="ib" style:--pct={`${u.percent}%`}>
    {#if u.stage === 'checking'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="update" />{/if}
    {#if u.stage === 'downloading'}<span class="ring" aria-hidden="true"></span>{/if}
    {#if view.pip && variant === 'icon'}<span class={['pip', view.pip]} aria-hidden="true"></span>{/if}
  </span>
  {#if variant !== 'icon'}<span class="lbl">{view.text}</span>{/if}
  {#if view.pip && variant !== 'icon'}<span class={['pip', 'end', view.pip]} aria-hidden="true"></span>{/if}
</button>

<style>
  .upd {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 12px;
    border-radius: var(--r-md);
    border: 2px solid var(--line);
    background: var(--surface);
    color: var(--text-2);
    font: 700 14.5px var(--f-body);
    text-align: left;
    cursor: pointer;
  }
  .upd:hover { color: var(--text); border-color: var(--line-strong); }
  .upd.primary { background: var(--primary-soft); border-color: var(--primary); color: var(--primary-ink); }
  .upd.ok { background: var(--ok-soft); border-color: var(--ok); color: var(--ok); }
  .upd.bad { color: var(--bad); }
  .lbl { flex: 1; min-width: 0; }
  .ib { position: relative; display: grid; place-items: center; width: 24px; height: 24px; flex: none; }
  .ring {
    position: absolute;
    inset: -4px;
    border-radius: 50%;
    background: conic-gradient(var(--primary) 0 var(--pct), var(--surface-3) 0);
    -webkit-mask: radial-gradient(closest-side, transparent calc(100% - 3px), #000 calc(100% - 2px));
    mask: radial-gradient(closest-side, transparent calc(100% - 3px), #000 calc(100% - 2px));
  }
  .pip { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .ib .pip { position: absolute; top: -3px; right: -3px; box-shadow: 0 0 0 2px var(--surface); }
  .pip.primary { background: var(--primary); }
  .pip.ok { background: var(--ok); }
  .pip.bad { background: var(--bad); }
  .spin { width: 18px; height: 18px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--primary); animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  /* Rail: a square icon button. */
  .upd.icon { justify-content: center; padding: 0; min-height: 48px; width: 48px; align-self: center; }
  /* More list: a full-width list row like its neighbours. */
  .upd.item { width: 100%; min-height: 56px; padding: 0 16px; gap: 14px; border: 0; border-top: 1px solid var(--line); border-radius: 0; font-size: 16px; color: var(--text); background: transparent; }
  .upd.item.primary, .upd.item.ok { border-color: var(--line); }
</style>
