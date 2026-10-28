<script lang="ts">
  import { t } from '../i18n.svelte';
  import Icon from '../ui/Icon.svelte';
  import { roleName } from './settings.svelte';

  interface Props {
    /** Accessible name of the group. */
    label: string;
    value: string[];
    /** Roles offered (built-in ones first). */
    options: string[];
    /** Small chips without the "add a group" field (matrix cells). */
    compact?: boolean;
    onchange: (v: string[]) => void;
  }
  let { label, value, options, compact = false, onchange }: Props = $props();
  const uid = $props.id();
  let draft = $state('');
  const all = $derived([...new Set([...options, ...value])]);

  function toggle(r: string) {
    onchange(value.includes(r) ? value.filter((x) => x !== r) : [...value, r]);
  }
  function add(e: SubmitEvent) {
    e.preventDefault();
    const r = draft.trim();
    draft = '';
    if (r && !value.includes(r)) onchange([...value, r]);
  }
</script>

<div class={['roles', { compact }]} role="group" aria-label={label}>
  {#each all as r (r)}
    <button type="button" class="chip" aria-pressed={value.includes(r)} onclick={() => toggle(r)}>
      {#if value.includes(r)}<Icon name="check" size={14} />{/if}{roleName(r)}
    </button>
  {/each}
  {#if !compact}
    <form class="add" onsubmit={add}>
      <label class="sr" for={uid}>{t('modSettings.roleAddLabel', { name: label })}</label>
      <input id={uid} class="inp" placeholder={t('modSettings.rolePlaceholder')} bind:value={draft} autocomplete="off" maxlength="40" />
      <button type="submit" class="chip plus" disabled={!draft.trim()} aria-label={t('modSettings.roleAdd')}><Icon name="plus" size={14} /></button>
    </form>
  {/if}
</div>

<style>
  .roles { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 36px;
    padding: 0 12px;
    border-radius: 999px;
    border: 2px solid var(--line);
    background: var(--surface);
    color: var(--text-2);
    font: 700 13.5px var(--f-body);
    cursor: pointer;
    white-space: nowrap;
  }
  .chip[aria-pressed='true'] { background: var(--primary-soft); border-color: var(--primary-edge); color: var(--primary-ink); }
  .chip:disabled { opacity: 0.5; cursor: not-allowed; }
  .compact .chip { min-height: 30px; padding: 0 9px; font-size: 12.5px; }
  .add { display: inline-flex; gap: 4px; align-items: center; }
  .inp {
    width: 130px;
    min-height: 36px;
    padding: 0 10px;
    border: 2px dashed var(--line-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font: 14px var(--f-body);
  }
  .inp::placeholder { color: var(--text-3); }
  .plus { padding: 0 9px; }
</style>
