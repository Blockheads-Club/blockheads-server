<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../i18n.svelte';
  import Badge from './Badge.svelte';

  interface Props {
    title: string;
    hint?: string;
    /** Applies the next time the server starts. */
    restart?: boolean;
    mono?: boolean;
    titleId?: string;
    children?: Snippet;
  }
  let { title, hint, restart = false, mono = false, titleId, children }: Props = $props();
</script>

<div class="row">
  <div class="row-l">
    <p class="t" id={titleId}>{title} {#if restart}<Badge tone="warn">{t('settings.restart')}</Badge>{/if}</p>
    {#if hint}<p class="hint" class:mono>{hint}</p>{/if}
  </div>
  {#if children}<div class="ctl">{@render children()}</div>{/if}
</div>

<style>
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 14px 0;
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .row-l { flex: 1 1 260px; min-width: 0; }
  .t { font-weight: 700; }
  .hint.mono { font-size: 13px; overflow-wrap: anywhere; }
  .ctl { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; max-width: 100%; }
</style>
