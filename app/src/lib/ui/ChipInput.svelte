<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import Button from './Button.svelte';

  interface Props {
    /** Accessible label of the add field (also its placeholder). */
    label: string;
    placeholder: string;
    addLabel: string;
    addIcon?: IconName;
    entries: string[];
    /** Accessible name of each remove button. */
    removeLabel: (entry: string) => string;
    /** Visible text on the remove button; an X icon when omitted. */
    removeText?: string;
    emptyText: string;
    limit?: number;
    disabled?: boolean;
    onadd: (name: string) => void;
    onremove: (name: string) => void;
  }
  let {
    label,
    placeholder,
    addLabel,
    addIcon = 'plus',
    entries,
    removeLabel,
    removeText,
    emptyText,
    limit = 5,
    disabled = false,
    onadd,
    onremove
  }: Props = $props();
  const uid = $props.id();
  let draft = $state('');
  let all = $state(false);
  const shown = $derived(all ? entries : entries.slice(0, limit));

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const n = draft.trim();
    draft = '';
    if (n) onadd(n);
  }
</script>

<form class="add" onsubmit={submit}>
  <label class="sr" for={uid}>{label}</label>
  <input id={uid} class="inp" {placeholder} bind:value={draft} autocomplete="off" {disabled} />
  <Button size="sm" icon={addIcon} type="submit" disabled={disabled || !draft.trim()}>{addLabel}</Button>
</form>

{#if entries.length === 0}
  <p class="hint empty">{emptyText}</p>
{:else}
  <ul class="ents">
    {#each shown as n (n)}
      <li class="ent">
        <span class="mono name" title={n}>{n}</span>
        {#if removeText}
          <Button variant="ghost" size="sm" label={removeLabel(n)} onclick={() => onremove(n)} {disabled}>{removeText}</Button>
        {:else}
          <Button variant="ghost" size="sm" icon="x" label={removeLabel(n)} onclick={() => onremove(n)} {disabled} />
        {/if}
      </li>
    {/each}
  </ul>
  {#if entries.length > limit}
    <button class="more" type="button" onclick={() => (all = !all)}>
      {all ? t('chips.less') : t('chips.more', { n: entries.length - limit })}
    </button>
  {/if}
{/if}

<style>
  .add { display: flex; gap: 8px; }
  .inp {
    flex: 1;
    min-width: 0;
    min-height: 44px;
    padding: 0 14px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 15px var(--f-mono);
  }
  .inp::placeholder { font-family: var(--f-body); color: var(--text-3); }
  .inp:disabled { opacity: 0.55; }
  .ents { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .ent {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 0 6px 0 12px;
    border-radius: var(--r-md);
    background: var(--surface-2);
  }
  .name { flex: 1; font-weight: 600; font-size: 14.5px; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .empty { padding: 4px 2px; }
  .more {
    align-self: flex-start;
    border: 0;
    background: none;
    min-height: 40px;
    padding: 0;
    color: var(--primary-ink);
    font: 700 14.5px var(--f-body);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
</style>
