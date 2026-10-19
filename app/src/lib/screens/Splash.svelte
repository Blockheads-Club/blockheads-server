<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { SplashState } from '../types';

  /**
   * The start-up screen. `card` is the whole content of the 600x380 splash window (desktop); `phone` is the
   * in-app screen that follows the Android system splash and only shows when the boot fails.
   * It is dark in every theme, like the system splash.
   */
  let { state, variant = 'card', square = false, canChoose = false, detail = '', onretry, onchoose, onquit }: {
    state: SplashState;
    variant?: 'card' | 'phone';
    /** No transparency (macOS): the window itself is the card, so no rounded corners. */
    square?: boolean;
    /** Desktop only: offer a folder picker when the data folder cannot be opened. */
    canChoose?: boolean;
    /** Why the last action failed, shown under the message. */
    detail?: string;
    onretry: () => void;
    onchoose?: () => void;
    onquit?: () => void;
  } = $props();
  const failed = $derived(state.boot === 'error');
  const phone = $derived(variant === 'phone');
  const shown = $derived(detail || state.error);
</script>

<div class={['wrap', phone && 'ph']} data-tauri-drag-region>
  <section class={['sp', 'bg', phone ? 'phone' : 'card', square && 'sq']} role="status" aria-live="polite" aria-label={t('splash.label')} data-tauri-drag-region>
    {#if phone}
      <img class="ring ic" src="/server-icon-adaptive.svg" alt="" width="168" height="168" />
    {:else}
      <img class="ic" data-tauri-drag-region src="/server-icon.svg" alt="" width="120" height="120" />
    {/if}
    {#if !phone}<div class="wm" data-tauri-drag-region>Blockheads<span class="tg">Server</span></div>{/if}

    {#if failed}
      <div class={['fail', phone && 'abs']} role="alert" data-tauri-drag-region>
        <p class="msg err" data-tauri-drag-region>{phone ? t('splash.errorShort') : t('splash.error')}</p>
        {#if shown}<p class="detail" title={shown} data-tauri-drag-region>{shown}</p>{/if}
        <div class="acts">
          <button type="button" class={['btn', 'b1', phone && 'tall']} onclick={() => onretry()}>{t('splash.retry')}</button>
          {#if canChoose && !phone}<button type="button" class="btn b2" onclick={() => onchoose?.()}>{t('splash.choose')}</button>{/if}
          {#if !phone}<button type="button" class="btn b3" onclick={() => onquit?.()}>{t('splash.quit')}</button>{/if}
        </div>
      </div>
    {:else if !phone}
      <div class="rack" aria-hidden="true" data-tauri-drag-region><span class="led on"></span><span class="led on"></span><span class="led on"></span></div>
      <p class="msg" data-tauri-drag-region>{state.boot === 'slow' ? t('splash.slow') : t('splash.loading')}</p>
    {/if}

    {#if phone}
      <div class="wm small" aria-hidden="true">Blockheads<span class="tg">Server</span></div>
    {:else}
      <div class="foot" data-tauri-drag-region><span>{t('splash.footer')}</span><span class="mono">{state.version}</span></div>
    {/if}
  </section>
</div>

<style>
  .wrap { height: 100vh; height: 100dvh; }
  .bg {
    background-color: #14110f;
    background-image: repeating-conic-gradient(from -3deg at 50% 118%, rgba(255, 210, 122, 0.1) 0deg 7deg, transparent 7deg 22deg);
    color: #f5ede5;
  }
  .sp { width: 100%; height: 100%; box-sizing: border-box; overflow: hidden; position: relative; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; text-align: center; font-family: var(--f-body); }
  .sp p { margin: 0; }
  .card { border-radius: 18px; padding: 16px 24px 44px; gap: 12px; }
  .card.sq { border-radius: 0; }
  .ic { animation: drop 0.6s cubic-bezier(0.2, 0.9, 0.3, 1.2) both; }
  .card .ic { border-radius: 28px; box-shadow: 0 8px 0 #000; }
  /* Phone (design: Android 12+ system splash): the adaptive icon in a circle, as the system splash before it. */
  .ring { width: 168px; height: 168px; border-radius: 50%; box-shadow: 0 6px 0 #1e7a4c; }
  @keyframes drop {
    from { transform: translateY(-28px) rotate(-10deg); opacity: 0; }
    to { transform: translateY(0) rotate(-4deg); opacity: 1; }
  }
  .wm { display: flex; align-items: center; gap: 8px; font: 800 30px/1 var(--f-display); letter-spacing: -0.02em; }
  .wm.small { position: absolute; left: 0; right: 0; bottom: 40px; justify-content: center; font-size: 20px; }
  .tg { display: inline-block; padding: 3px 8px 4px; border-radius: 7px; transform: rotate(-6deg); font-size: 24px; background: #56c990; color: #0e2a1c; box-shadow: 0 3px 0 #1e7a4c; }
  .small .tg { font-size: 16px; }
  .rack { display: flex; gap: 6px; align-items: center; margin-top: 4px; }
  .led { width: 8px; height: 8px; border-radius: 50%; background: #3a312b; }
  .led.on { background: #56c990; box-shadow: 0 0 8px #56c990; animation: led 1.2s ease-in-out infinite; }
  .led.on:nth-child(2) { animation-delay: 0.2s; }
  .led.on:nth-child(3) { animation-delay: 0.4s; }
  @keyframes led { 50% { opacity: 0.25; } }
  .msg { font-size: 14.5px; color: #cbbeb2; }
  .fail { display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: 440px; }
  .fail.abs { position: absolute; left: 20px; right: 20px; bottom: 110px; max-width: none; }
  .err { font-size: 15px; font-weight: 700; color: #ffd2cb; }
  .detail { font: 400 12px/1.4 var(--f-mono); color: #ae9f92; overflow-wrap: anywhere; max-width: 440px; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; margin-top: 4px; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: 8px; min-height: 40px; padding: 0 14px; border-radius: 10px; font: 700 14px var(--f-body); cursor: pointer; }
  .btn.tall { min-height: 44px; }
  .btn:focus-visible { outline: 3px solid #86b9ff; outline-offset: 2px; }
  .b1 { background: #ffd27a; color: #1f1a17; border: 2px solid #1f1a17; box-shadow: 0 3px 0 #c98a2e; }
  .b1:active { transform: translateY(2px); box-shadow: 0 1px 0 #c98a2e; }
  .b2 { background: #2a231f; color: #f5ede5; border: 2px solid #54483f; }
  .b3 { background: transparent; color: #cbbeb2; border: 2px solid transparent; }
  .b2:hover, .b3:hover { background: #312924; }
  .foot { position: absolute; left: 18px; right: 18px; bottom: 14px; display: flex; justify-content: space-between; font-size: 12.5px; color: #ae9f92; }
  .mono { font-family: var(--f-mono); }
  @media (prefers-reduced-motion: reduce) {
    .sp :global(*) { animation: none !important; }
  }
</style>
