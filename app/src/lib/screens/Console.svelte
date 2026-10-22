<script lang="ts">
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import ConsoleView from '../ui/ConsoleView.svelte';
  import { clock } from '../util';
  import { viewport } from '../viewport.svelte';

  let paused = $state(false);
  /** A search handed over by another screen (a mod's "Open in console"), used once. */
  const initialQuery = store.consoleQuery;
  store.consoleQuery = '';

  /** Download the buffered log as a text file. */
  function save() {
    const text = store.log.map((e) => `${clock(e.at)}  ${e.text}`).join('\n');
    const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = 'blockheads-server.log';
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="screen">
  <PageHeader title={t('nav.console')} compact>
    {#snippet actions()}
      {#if !viewport.phone}
        <span class="hint live">{store.active ? t('console.live') : t('console.idle')} · {t('console.lines', { n: store.log.length })}</span>
      {/if}
      <Button size="sm" icon="pause" pressed={paused} disabled={!store.active} onclick={() => (paused = !paused)}>{t('console.pause')}</Button>
      <Button size="sm" icon="download" disabled={store.log.length === 0} onclick={save}>{t('console.save')}</Button>
    {/snippet}
  </PageHeader>
  <ConsoleView
    entries={store.log}
    bind:paused
    active={store.state === 'running'}
    history={store.history}
    phone={viewport.phone}
    {initialQuery}
    onsend={(line) => store.sendCommand(line)}
  />
</div>

<style>
  .screen { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .live { align-self: center; }
</style>
