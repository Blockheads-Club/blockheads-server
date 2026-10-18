<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  interface Props {
    icon: IconName;
    title: string;
    text?: string;
    tone?: 'sun' | 'quiet';
    children?: Snippet;
  }
  let { icon, title, text, tone = 'sun', children }: Props = $props();
</script>

<div class="empty">
  <span class={['blob', tone]}><Icon name={icon} size={36} /></span>
  <h2>{title}</h2>
  {#if text}<p class="muted">{text}</p>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 14px; padding: 48px 24px; }
  h2 { font-size: 24px; }
  p { max-width: 440px; }
  .blob {
    width: 84px;
    height: 84px;
    border-radius: 22px;
    display: grid;
    place-items: center;
    transform: rotate(-6deg);
  }
  .sun { background: var(--sun); color: #1f1a17; box-shadow: 0 4px 0 #c98a2e; }
  .quiet { background: var(--surface-3); color: var(--text-2); }
  .actions { display: flex; gap: 10px; flex-wrap: wrap; justify-content: center; }
</style>
