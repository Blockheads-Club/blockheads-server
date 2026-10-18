<script lang="ts">
  import { tick } from 'svelte';
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import { viewport } from '../viewport.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  interface Item {
    id: string;
    label: string;
    icon?: IconName;
    tone?: 'bad';
    separated?: boolean;
  }
  let { label, items, onselect }: { label: string; items: Item[]; onselect: (id: string) => void } = $props();
  let open = $state(false);
  let root: HTMLElement | undefined = $state();

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      root?.querySelector<HTMLElement>('[role=menuitem]')?.focus();
    }
  }

  function close(refocus = true) {
    open = false;
    if (refocus) root?.querySelector<HTMLElement>('.trigger button')?.focus();
  }

  function keydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
      return;
    }
    const els = [...root!.querySelectorAll<HTMLElement>('[role=menuitem]')];
    const at = els.indexOf(document.activeElement as HTMLElement);
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (step) {
      e.preventDefault();
      els[(at + step + els.length) % els.length].focus();
    } else if (e.key === 'Tab') {
      close(false);
    }
  }

  function pick(id: string) {
    close();
    onselect(id);
  }
</script>

<svelte:window onmousedown={(e) => open && !root?.contains(e.target as Node) && close(false)} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="menu-root" bind:this={root} onkeydown={keydown}>
  <span class="trigger">
    <Button variant="ghost" size="sm" icon="more" {label} expanded={open} haspopup onclick={toggle} />
  </span>
  {#if open}
    {#if viewport.phone}<span class="scrim" aria-hidden="true"></span>{/if}
    <div class={["menu", viewport.phone && "sheet"]} role="menu" aria-label={label}>
      {#if viewport.phone}<span class="grab"></span><p class="sheet-title">{label}</p>{/if}
      {#each items as it (it.id)}
        {#if it.separated}<span class="sep" role="separator"></span>{/if}
        <button class={['mi', it.tone]} role="menuitem" type="button" tabindex="-1" onclick={() => pick(it.id)}>
          {#if it.icon}<Icon name={it.icon} />{/if}{it.label}
        </button>
      {/each}
      {#if viewport.phone}<span class="cancel"><Button block onclick={() => close()}>{t('common.cancel')}</Button></span>{/if}
    </div>
  {/if}
</div>

<style>
  .menu-root { position: relative; display: inline-flex; }
  .menu {
    position: absolute;
    right: 0;
    top: 46px;
    z-index: 20;
    background: var(--surface);
    border: 2px solid var(--line-strong);
    border-radius: 14px;
    box-shadow: var(--lift);
    padding: 6px;
    display: flex;
    flex-direction: column;
    min-width: 220px;
  }
  .mi {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 12px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text);
    font: 500 15px var(--f-body);
    text-align: left;
    cursor: pointer;
    width: 100%;
  }
  .mi:hover { background: var(--surface-2); }
  .mi.bad { color: var(--bad); font-weight: 700; }
  .scrim { position: fixed; inset: 0; z-index: 19; background: var(--scrim); }
  .sheet {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    top: auto;
    z-index: 20;
    border-radius: 24px 24px 0 0;
    border-width: 2px 0 0;
    padding: 10px 16px calc(24px + env(safe-area-inset-bottom));
  }
  .sheet .mi { min-height: 56px; font-size: 16px; font-weight: 600; }
  .grab { width: 40px; height: 5px; border-radius: 999px; background: var(--line-strong); align-self: center; margin-bottom: 10px; }
  .sheet-title { font-weight: 700; padding: 0 8px 8px; border-bottom: 2px solid var(--line); margin-bottom: 4px; }
  .cancel { margin-top: 8px; }
  .sep { height: 1px; background: var(--line); margin: 4px 6px; }
</style>
