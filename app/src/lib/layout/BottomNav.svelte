<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Icon from '../ui/Icon.svelte';
  import { PHONE_NAV } from './nav';

  /** The "More" item also lights up for the screens it contains. */
  const current = $derived(store.screen === 'console' || store.screen === 'access' || store.screen === 'settings' ? 'more' : store.screen);
</script>

<nav class="bnav" aria-label={t('nav.aria')}>
  {#each PHONE_NAV as it (it.id)}
    <button class="bn" aria-current={current === it.id ? 'page' : undefined} onclick={() => store.navigate(it.id)}>
      <span class="pill"><Icon name={it.icon} /></span>
      {t(`nav.${it.id}`)}
    </button>
  {/each}
</nav>

<style>
  .bnav {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    border-top: 2px solid var(--line);
    background: var(--surface);
    padding: 6px 4px calc(6px + env(safe-area-inset-bottom));
    position: sticky;
    bottom: 0;
    z-index: 10;
  }
  .bn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-height: 56px;
    justify-content: center;
    border: 0;
    background: transparent;
    color: var(--text-3);
    font: 700 11.5px var(--f-body);
    border-radius: 12px;
    cursor: pointer;
  }
  .pill { width: 56px; height: 30px; border-radius: 999px; display: grid; place-items: center; }
  .bn[aria-current='page'] { color: var(--text); }
  .bn[aria-current='page'] .pill { background: var(--primary-soft); color: var(--primary-ink); }
</style>
