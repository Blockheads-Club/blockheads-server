<script lang="ts" generics="T extends string">
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  interface Item {
    id: T;
    label: string;
    icon?: IconName;
    dot?: string;
  }
  interface Props {
    items: Item[];
    value: T;
    label: string;
    /** `tablist` for views that switch a panel; `radiogroup` for choices (theme, size). */
    mode?: 'tablist' | 'radiogroup';
    onchange: (id: T) => void;
  }
  let { items, value, label, mode = 'tablist', onchange }: Props = $props();
  const uid = $props.id();

  function key(e: KeyboardEvent, i: number) {
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = (i + step + items.length) % items.length;
    onchange(items[next].id);
    document.getElementById(`${uid}-${items[next].id}`)?.focus();
  }
</script>

<div class="tabs" role={mode} aria-label={label}>
  {#each items as it, i (it.id)}
    <button
      id={`${uid}-${it.id}`}
      class="tab"
      type="button"
      role={mode === 'tablist' ? 'tab' : 'radio'}
      aria-selected={mode === 'tablist' ? value === it.id : undefined}
      aria-checked={mode === 'radiogroup' ? value === it.id : undefined}
      tabindex={value === it.id ? 0 : -1}
      onclick={() => onchange(it.id)}
      onkeydown={(e) => key(e, i)}
    >
      {#if it.dot}<span class="dot" style:color={it.dot}></span>{/if}
      {#if it.icon}<Icon name={it.icon} size={18} />{/if}
      <span>{it.label}</span>
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px;
    background: var(--surface-2);
    border: 2px solid var(--line);
    border-radius: 14px;
    max-width: 100%;
  }
  .tab {
    min-height: 40px;
    padding: 0 14px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--text-2);
    font: 700 14px/1 var(--f-body);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    cursor: pointer;
    transition: background-color var(--dur-fast);
  }
  .tab[aria-selected='true'],
  .tab[aria-checked='true'] {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 2px 0 var(--line-strong);
  }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
</style>
