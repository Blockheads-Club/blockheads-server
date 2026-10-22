<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from '../ui/Icon.svelte';

  /** ServerNotice: a banner that grows downwards, actions below the text. Errors are alerts, the rest status. */
  interface Props {
    tone: 'info' | 'warn' | 'bad';
    title: string;
    icon?: IconName;
    role?: 'status' | 'alert' | 'note';
    children?: Snippet;
    actions?: Snippet;
  }
  let { tone, title, icon, role, children, actions }: Props = $props();
  const defaults = { info: 'info', warn: 'alert', bad: 'x-circle' } as const;
</script>

<div class={['notice', tone]} role={role ?? (tone === 'bad' ? 'alert' : 'status')}>
  <span class="ic"><Icon name={icon ?? defaults[tone]} /></span>
  <div class="nb">
    <p class="title">{title}</p>
    {#if children}<div class="muted text">{@render children()}</div>{/if}
    {#if actions}<div class="na">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .notice { display: flex; gap: 14px; align-items: flex-start; padding: 14px 16px; border-radius: 14px; border: 2px solid transparent; }
  .info { background: var(--info-soft); border-color: color-mix(in oklab, var(--info), transparent 65%); }
  .warn { background: var(--warn-soft); border-color: color-mix(in oklab, var(--warn), transparent 60%); }
  .bad { background: var(--bad-soft); border-color: color-mix(in oklab, var(--bad), transparent 60%); }
  .ic { display: flex; margin-top: 1px; }
  .info .ic { color: var(--info); }
  .warn .ic { color: var(--warn); }
  .bad .ic { color: var(--bad); }
  .nb { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .title { font-weight: 700; }
  .text { font-size: 14.5px; display: flex; flex-direction: column; gap: 4px; }
  .text :global(p) { margin: 0; }
  .na { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; margin-top: 6px; }
</style>
