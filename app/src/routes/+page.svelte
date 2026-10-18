<script lang="ts">
  import { onMount } from 'svelte';
  import { api, win } from '$lib/api';
  import { i18n, t } from '$lib/i18n.svelte';
  import BottomNav from '$lib/layout/BottomNav.svelte';
  import Sidebar from '$lib/layout/Sidebar.svelte';
  import TitleBar from '$lib/layout/TitleBar.svelte';
  import Access from '$lib/screens/Access.svelte';
  import Console from '$lib/screens/Console.svelte';
  import Dashboard from '$lib/screens/Dashboard.svelte';
  import More from '$lib/screens/More.svelte';
  import Players from '$lib/screens/Players.svelte';
  import Settings from '$lib/screens/Settings.svelte';
  import Splash from '$lib/screens/Splash.svelte';
  import Worlds from '$lib/screens/Worlds.svelte';
  import { store } from '$lib/store.svelte';
  import { initTheme } from '$lib/theme.svelte';
  import Alert from '$lib/ui/Alert.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Toasts from '$lib/ui/Toasts.svelte';
  import { viewport, watchViewport } from '$lib/viewport.svelte';

  /**
   * Desktop: the main window starts hidden and the separate splash window (/splash) mirrors the boot state; once
   * the boot work is done the backend shows this window and closes the splash. Android: the system splash is up
   * until this page draws; if the boot fails, the error screen below follows it.
   */
  const mobile = $derived(store.info ? store.info.platform === 'android' : /Android/i.test(navigator.userAgent));
  const failure = $derived(store.boot === 'error' && mobile);
  $effect(() => {
    if (store.boot === 'done') return;
    const state = { boot: store.boot, error: store.bootError, version: store.info?.version ?? '', lang: i18n.lang };
    void api.splashSet(state).catch(() => {});
  });
  let revealed = false;
  $effect(() => {
    if (store.boot !== 'done' || revealed) return;
    revealed = true;
    void api.mainReady().catch(() => {});
  });

  /** The custom title bar and rounded frame: desktop Tauri only (Android keeps its layout). */
  const os = $derived.by(() => {
    const p = store.info?.platform;
    return p === 'windows' || p === 'macos' || p === 'linux' ? p : null;
  });
  $effect(() => {
    document.documentElement.classList.toggle('desktop', os !== null);
  });
  const ctx = $derived(t(`nav.${store.screen}`));
  const run = $derived.by(() => {
    if (store.state === 'stopped') return null;
    const tone = store.state === 'failed' ? 'bad' : store.state === 'running' ? 'ok' : 'warn';
    const label = t(`run.state.${store.state}`);
    return { label: store.currentWorld && store.state !== 'failed' ? `${label} · ${store.currentWorld.name}` : label, tone } as const;
  });
  $effect(() => {
    if (os) document.title = `${t('app.title')} · ${ctx}`;
  });
  /** Maximized or fullscreen: the window fills the screen, so no rounded corners. */
  let square = $state(false);
  const round = $derived(os !== null && os !== 'macos' && !square);

  onMount(() => {
    initTheme();
    const syncSquare = () => void Promise.all([win.isMaximized(), win.isFullscreen()]).then(([m, f]) => (square = m || f)).catch(() => {});
    syncSquare();
    let offResize: (() => void) | undefined;
    void win.onResized(syncSquare).then((u) => (offResize = u)).catch(() => {});
    const unwatch = watchViewport();
    let offRetry: (() => void) | undefined;
    void api.onSplashRetry(() => void store.init()).then((u) => (offRetry = u)).catch(() => {});
    void store.init();
    return () => {
      unwatch();
      offResize?.();
      offRetry?.();
      store.stopPolling();
    };
  });
</script>

{#if failure}
  <Splash variant="phone" state={{ boot: 'error', error: store.bootError, version: store.info?.version ?? '', lang: i18n.lang }} onretry={() => void store.init()} />
{:else}
<a class="skip" href="#main">{t('common.skip')}</a>
{#snippet shell()}
<div class="app" class:phone={viewport.phone} class:desk={os !== null}>
  {#if !viewport.phone}<Sidebar rail={viewport.rail} />{/if}
  <div class="content" class:fill={store.screen === 'console'}>
    <main id="main" tabindex="-1">
      {#if store.error}
        <div class="err" data-testid="error">
          <Alert tone="bad" title={store.error}>
            {#snippet actions()}<Button size="sm" variant="ghost" onclick={() => (store.error = '')}>{t('common.close')}</Button>{/snippet}
          </Alert>
        </div>
      {/if}
      {#if !store.ready}
        <div class="loading" role="status" aria-busy="true">
          <span class="spin" aria-hidden="true"></span>
          <p class="muted">{t('common.loading')}</p>
          <div class="skel" style="width:60%;height:14px"></div>
          <div class="skel" style="width:35%;height:12px"></div>
        </div>
      {:else if store.screen === 'dashboard'}
        <Dashboard />
      {:else if store.screen === 'worlds'}
        <Worlds />
      {:else if store.screen === 'console'}
        <Console />
      {:else if store.screen === 'players'}
        <Players />
      {:else if store.screen === 'access'}
        <Access />
      {:else if store.screen === 'settings'}
        <Settings />
      {:else}
        <More />
      {/if}
    </main>
    {#if viewport.phone}<BottomNav />{/if}
  </div>
</div>
{/snippet}
{#if os}
  <div class={['frame', round && 'round']}>
    <div class="win">
      <TitleBar {os} {ctx} {run} />
      <div class="body">{@render shell()}</div>
    </div>
    <Toasts />
  </div>
{:else}
  {@render shell()}
  <Toasts />
{/if}
{/if}

<style>
  .app { min-height: 100vh; display: flex; background: var(--bg); }
  /* Desktop: a fixed-height shell, the sidebar and title bar stay put and only the content area scrolls. */
  .frame { position: relative; height: 100vh; height: 100dvh; }
  .frame.round { clip-path: inset(0 round 12px); }
  .frame.round::after {
    content: ''; position: fixed; inset: 0; border: 1px solid var(--line-strong); border-radius: 12px;
    pointer-events: none; z-index: 1000;
  }
  .win { height: 100%; display: flex; flex-direction: column; background: var(--bg); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .app.desk { flex: 1; min-height: 0; height: 100%; overflow: hidden; }
  .desk .content { overflow-y: auto; }
  .desk .content.fill { height: auto; overflow: hidden; }
  .content { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .content.fill { height: 100vh; height: 100dvh; }
  main { flex: 1; display: flex; flex-direction: column; min-height: 0; outline: none; }
  .err { padding: 16px 28px 0; position: sticky; top: 0; z-index: 5; }
  .loading { padding: 48px 28px; display: flex; flex-direction: column; gap: 12px; align-items: flex-start; }
  .spin {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 3px solid var(--surface-3);
    border-top-color: var(--primary);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .skel {
    background: linear-gradient(90deg, var(--surface-2), var(--surface-3), var(--surface-2));
    background-size: 200% 100%;
    animation: sk 1.4s ease-in-out infinite;
    border-radius: 8px;
  }
  @keyframes sk { to { background-position: -200% 0; } }
  .phone .err { padding: 8px 16px 0; }
</style>
