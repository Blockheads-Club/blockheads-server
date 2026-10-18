<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { RunState } from '../types';
  import Alert from './Alert.svelte';
  import Button from './Button.svelte';
  import Card from './Card.svelte';

  /** Offline / failed / loading banners shown above the dashboard tiles. */
  interface Props {
    state: RunState;
    worldName: string | undefined;
    detail: string | null;
    offline: boolean;
    onretry: () => void;
    onport: () => void;
    onlog: () => void;
  }
  let { state, worldName, detail, offline, onretry, onport, onlog }: Props = $props();
</script>

{#if offline}
  <Alert tone="bad" title={t('hero.offline')} icon="wifi">
    {t('hero.offlineText')}
    {#snippet actions()}<Button size="sm" icon="refresh" onclick={onretry}>{t('hero.retryNow')}</Button>{/snippet}
  </Alert>
{/if}
{#if state === 'failed'}
  <Alert tone="bad" title={t('hero.failed')}>
    {detail ?? t('hero.failedText')}
    {#snippet actions()}
      <Button size="sm" onclick={onport}>{t('hero.changePort')}</Button>
      <Button size="sm" variant="ghost" onclick={onlog}>{t('hero.openLog')}</Button>
    {/snippet}
  </Alert>
{/if}
{#if state === 'starting'}
  <Card label={t('hero.loadingWorld', { name: worldName ?? '' })}>
    <div class="loading" role="status">
      <p class="strong">{t('hero.loadingWorld', { name: worldName ?? '' })}</p>
      <div class="bar-track"><span class="ind"></span></div>
    </div>
  </Card>
{/if}

<style>
  .strong { font-weight: 700; }
  .loading { display: flex; flex-direction: column; gap: 10px; }
  .bar-track { height: 10px; border-radius: 999px; background: var(--surface-3); overflow: hidden; }
  .ind {
    display: block;
    height: 100%;
    width: 40%;
    background: var(--primary);
    border-radius: 999px;
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
</style>
