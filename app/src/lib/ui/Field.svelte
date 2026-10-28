<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  interface Props {
    label: string;
    value?: string;
    hint?: string;
    error?: string;
    type?: 'text' | 'password' | 'number' | 'search';
    mono?: boolean;
    icon?: IconName;
    placeholder?: string;
    maxlength?: number;
    min?: number;
    max?: number;
    step?: number | 'any';
    disabled?: boolean;
    hideLabel?: boolean;
    autocomplete?: 'off' | 'on';
    inputmode?: 'numeric' | 'text';
    center?: boolean;
    required?: boolean;
    width?: string;
    trailing?: Snippet;
    oninput?: (v: string) => void;
  }
  let {
    label,
    value = $bindable(''),
    hint,
    error,
    type = 'text',
    mono = false,
    icon,
    placeholder,
    maxlength,
    min,
    max,
    step,
    disabled = false,
    hideLabel = false,
    autocomplete = 'off',
    inputmode,
    center = false,
    required = false,
    width,
    trailing,
    oninput
  }: Props = $props();
  const uid = $props.id();
  const describedby = $derived(error ? `${uid}-e` : hint ? `${uid}-h` : undefined);
</script>

<div class="field" style:width>
  <label class="lbl" class:sr={hideLabel} for={uid}>{label}</label>
  <div class="wrap">
    {#if icon}<span class="lead"><Icon name={icon} /></span>{/if}
    <input
      id={uid}
      class="inp"
      class:mono
      class:center
      class:with-icon={icon}
      {type}
      {placeholder}
      {maxlength}
      {min}
      {max}
      {step}
      {disabled}
      {required}
      {autocomplete}
      {inputmode}
      value={value}
      aria-invalid={error ? 'true' : undefined}
      aria-describedby={describedby}
      oninput={(e) => {
        value = e.currentTarget.value;
        oninput?.(value);
      }}
    />
    {#if trailing}{@render trailing()}{/if}
  </div>
  {#if error}
    <p id="{uid}-e" class="err"><Icon name="alert" size={16} />{error}</p>
  {:else if hint}
    <p id="{uid}-h" class="hint">{hint}</p>
  {/if}
</div>

<style>
  .field { min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .lbl { display: block; font-weight: 700; font-size: 14.5px; color: var(--text); }
  .wrap { position: relative; display: flex; gap: 6px; align-items: center; }
  .lead { position: absolute; left: 12px; top: 12px; color: var(--text-3); display: flex; pointer-events: none; }
  .inp {
    width: 100%;
    min-height: 44px;
    padding: 0 14px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 16px/1.4 var(--f-body);
    min-width: 0;
  }
  .inp.mono { font-family: var(--f-mono); font-size: 15px; }
  .inp.center { text-align: center; }
  .inp.with-icon { padding-left: 42px; }
  .inp::placeholder { color: var(--text-3); font-family: var(--f-body); }
  .inp[aria-invalid='true'] { border-color: var(--bad); }
  .inp:disabled { opacity: 0.55; cursor: not-allowed; }
  .err { font-size: 13.5px; color: var(--bad); display: flex; gap: 6px; align-items: center; font-weight: 700; }
</style>
