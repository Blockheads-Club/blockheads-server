<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { errText, setLang } from '$lib/i18n.svelte';
  import Splash from '$lib/screens/Splash.svelte';
  import type { SplashState } from '$lib/types';

  /** Contents of the separate splash window (src-tauri opens it first); the hidden main window pushes the state. */
  let current = $state<SplashState | null>(null);
  let detail = $state('');
  const square = navigator.userAgent.includes('Mac');

  function apply(s: SplashState) {
    setLang(s.lang);
    detail = '';
    current = s;
  }

  async function choose() {
    try {
      await api.chooseDataFolder();
    } catch (e) {
      detail = errText(e);
    }
  }

  onMount(() => {
    let off: (() => void) | undefined;
    let gone = false;
    void api.onSplashState(apply).then((u) => (gone ? u() : (off = u))).catch(() => {});
    void api.splashGet().then((s) => s && !current && apply(s)).catch(() => {});
    return () => {
      gone = true;
      off?.();
    };
  });
</script>

<svelte:head><title>Blockheads Server</title></svelte:head>

{#if current}
  <Splash state={current} {square} {detail} canChoose onretry={() => void api.splashRetry()} onchoose={() => void choose()} onquit={() => void api.appQuit()} />
{/if}

<style>
  :global(html), :global(body) { background: transparent !important; overflow: hidden; }
</style>
