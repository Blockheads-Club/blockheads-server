<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { Json, SettingField } from '../types';
  import Button from '../ui/Button.svelte';
  import Field from '../ui/Field.svelte';
  import SelectField from '../ui/SelectField.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import RoleChips from './RoleChips.svelte';
  import SettingControl from './SettingControl.svelte';
  import { fieldText, itemFields, matrixRoles, newItem, problemText, roleName, roleOptions, setCell, validate } from './settings.svelte';

  interface Props {
    field: SettingField;
    value: Json;
    /** Accessible name of the control (the visible label sits next to it). */
    label: string;
    /** Error text shown under a text or number box. */
    error?: string;
    onchange: (v: Json) => void;
  }
  let { field, value, label, error, onchange }: Props = $props();

  const strs = (v: Json): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);
  const obj = (v: Json): Record<string, Json> => (v && typeof v === 'object' && !Array.isArray(v) ? v : {});
  const items = $derived(Array.isArray(value) ? value : []);
  const subs = $derived(itemFields(field));
  const cellRoles = $derived(roleOptions(field.default, ...matrixRoles(field.default), ...matrixRoles(value)));

  /** A number box: an empty or broken entry stays text so the rule check can name it. */
  function numberIn(s: string) {
    const n = Number(s);
    onchange(s.trim() === '' || Number.isNaN(n) ? s : n);
  }
  function cell(row: string, col: string): Json {
    return obj(obj(value)[row])[col] ?? (field.cell === 'roles' ? [] : false);
  }
  function setItem(i: number, key: string, v: Json) {
    onchange(items.map((it, j) => (j === i ? { ...obj(it), [key]: v } : it)));
  }
</script>

{#if field.type === 'bool'}
  <Toggle checked={value === true} {label} hideLabel onchange={(v) => onchange(v)} />
{:else if field.type === 'int' || field.type === 'float'}
  <Field
    {label}
    hideLabel
    type="number"
    inputmode={field.type === 'int' ? 'numeric' : undefined}
    value={String(value ?? '')}
    min={field.min}
    max={field.max}
    step={field.step ?? (field.type === 'int' ? 1 : 'any')}
    width="160px"
    {error}
    oninput={numberIn}
  />
{:else if field.type === 'enum'}
  <SelectField {label} hideLabel value={String(value)} options={(field.values ?? []).map((v) => ({ value: v, label: v }))} onchange={(v) => onchange(v)} />
{:else if field.type === 'string'}
  <Field {label} hideLabel value={String(value ?? '')} width="min(100%, 360px)" {error} oninput={(v) => onchange(v)} />
{:else if field.type === 'roles'}
  <RoleChips {label} value={strs(value)} options={roleOptions(field.default, value)} onchange={(v) => onchange(v)} />
{:else if field.type === 'matrix'}
  <div class="scroll">
    <table class="matrix">
      <thead>
        <tr><td></td>{#each field.cols ?? [] as c (c)}<th scope="col">{roleName(c)}</th>{/each}</tr>
      </thead>
      <tbody>
        {#each field.rows ?? [] as r (r)}
          <tr>
            <th scope="row">{roleName(r)}</th>
            {#each field.cols ?? [] as c (c)}
              {@const name = t('modSettings.cell', { row: roleName(r), col: roleName(c) })}
              <td>
                {#if field.cell === 'roles'}
                  <RoleChips label={name} compact value={strs(cell(r, c))} options={cellRoles} onchange={(v) => onchange(setCell(value, r, c, v))} />
                {:else}
                  <input type="checkbox" class="box" aria-label={name} checked={cell(r, c) === true} onchange={(e) => onchange(setCell(value, r, c, e.currentTarget.checked))} />
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <div class="list">
    {#if items.length === 0}<p class="hint">{t('modSettings.listEmpty')}</p>{/if}
    {#each items as it, i (i)}
      <fieldset class="item">
        <legend class="sr">{t('modSettings.listItem', { n: i + 1 })}</legend>
        <div class="ihead">
          <span class="num mono">{`#${i + 1}`}</span>
          <Button size="sm" variant="quiet-bad" icon="trash" label={t('modSettings.listRemove', { n: i + 1 })} onclick={() => onchange(items.filter((_, j) => j !== i))} />
        </div>
        {#each subs as s (s.key)}
          {@const sub = obj(it)[s.key] ?? s.default}
          {@const p = validate(s, sub)}
          {@const name = fieldText(s.label, s.key)}
          <div class="sub" class:wide={s.type === 'matrix' || s.type === 'list' || s.type === 'roles'}>
            <span class="sl" aria-hidden="true">{name}</span>
            <SettingControl field={s} value={sub} label={`${name} #${i + 1}`} error={p ? problemText(p, true) : undefined} onchange={(v) => setItem(i, s.key, v)} />
          </div>
        {/each}
      </fieldset>
    {/each}
    <div class="lfoot">
      <Button size="sm" icon="plus" disabled={field.max_items !== undefined && items.length >= field.max_items} onclick={() => onchange([...items, newItem(field)])}>{t('modSettings.listAdd')}</Button>
      {#if field.max_items !== undefined}<span class="hint">{t('modSettings.listCount', { n: items.length, max: field.max_items })}</span>{/if}
    </div>
  </div>
{/if}

<style>
  .scroll { overflow-x: auto; max-width: 100%; }
  .matrix { border-collapse: collapse; font-size: 14px; }
  .matrix th { font-weight: 700; text-align: left; padding: 6px 10px; white-space: nowrap; }
  .matrix thead th { color: var(--text-3); font-size: 13px; text-align: center; }
  .matrix td { padding: 6px 10px; text-align: center; border-top: 1px solid var(--line); vertical-align: middle; }
  .matrix tbody th { border-top: 1px solid var(--line); }
  .box { width: 22px; height: 22px; accent-color: var(--primary); cursor: pointer; }
  .list { display: flex; flex-direction: column; gap: 10px; width: 100%; }
  .item {
    margin: 0;
    padding: 10px 14px 14px;
    border: 2px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface-2);
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .ihead { display: flex; justify-content: space-between; align-items: center; }
  .num { color: var(--text-3); font-weight: 700; }
  .sub { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .sub.wide { flex-direction: column; align-items: stretch; }
  .sub.wide .sl { min-width: 0; }
  .sl { font-weight: 700; font-size: 14px; min-width: 120px; }
  .lfoot { display: flex; align-items: center; gap: 10px; }
</style>
