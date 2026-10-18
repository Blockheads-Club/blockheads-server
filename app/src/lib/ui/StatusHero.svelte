<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { RunState } from '../types';
  import Button from './Button.svelte';
  import Card from './Card.svelte';

  /** The state-dependent actions (start / stop / restart). On a phone they sit in a world card. */
  interface Props {
    state: RunState;
    worldName: string | undefined;
    phone?: boolean;
    onstart: () => void;
    onstop: () => void;
    onrestart: () => void;
  }
  let { state, worldName, phone = false, onstart, onstop, onrestart }: Props = $props();
</script>

{#snippet actions()}
  {#if state === 'running'}
    {#if !phone}<Button icon="refresh" onclick={onrestart}>{t('hero.restart')}</Button>{/if}
    <Button variant="danger" size={phone ? 'md' : 'lg'} icon="stop" block={phone} onclick={onstop}>{t('hero.stop')}</Button>
  {:else if state === 'starting'}
    <Button size={phone ? 'md' : 'lg'} loading block={phone}>{t('hero.starting')}</Button>
  {:else if state === 'stopping'}
    <Button size={phone ? 'md' : 'lg'} loading block={phone}>{t('hero.stopping')}</Button>
  {:else}
    <Button variant="primary" size={phone ? 'md' : 'lg'} icon="play" block={phone} disabled={!worldName} onclick={onstart}>
      {state === 'failed' ? t('hero.retry') : t('hero.start')}
    </Button>
  {/if}
{/snippet}

{#if phone}
  <Card>
    <p class="hint strong">{t('dash.world')}</p>
    <p class="wname">{worldName ?? t('dash.noWorld')}</p>
    <div class="phone-actions">{@render actions()}</div>
  </Card>
{:else}
  <div class="acts">{@render actions()}</div>
{/if}

<style>
  .strong { font-weight: 700; }
  .wname { font: 800 24px var(--f-display); margin: 2px 0 12px; overflow-wrap: anywhere; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; }
  .phone-actions { display: flex; }
</style>
