<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Column {
    key: string;
    label: string;
    /** Hidden visually (the column still has a name for screen readers). */
    srOnly?: boolean;
    /** Dropped on narrow screens. */
    narrow?: boolean;
    align?: 'end';
  }
  interface Props {
    label: string;
    columns: Column[];
    /** CSS grid-template-columns shared by the header and every row. */
    template: string;
    rows: Snippet;
  }
  let { label, columns, template, rows }: Props = $props();
</script>

<div class="card" role="table" aria-label={label} style:--cols={template}>
  <div class="tr th" role="row">
    {#each columns as c (c.key)}
      <span role="columnheader" class:narrow={c.narrow} class:end={c.align === 'end'}>
        {#if c.srOnly}<span class="sr">{c.label}</span>{:else}{c.label}{/if}
      </span>
    {/each}
  </div>
  {@render rows()}
</div>

<style>
  .card { background: var(--surface); border: 2px solid var(--line); border-radius: var(--r-lg); }
  .card :global(.tr) {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 14px;
    align-items: center;
    padding: 12px 18px;
    border-top: 1px solid var(--line);
  }
  .card :global(.tr.th) {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-3);
    background: var(--surface-2);
    border-top: 0;
    border-radius: 14px 14px 0 0;
    padding-top: 10px;
    padding-bottom: 10px;
  }
  .end { text-align: end; }
  @media (max-width: 760px) {
    .card :global(.tr) { grid-template-columns: 52px minmax(0, 1fr) auto; }
    .card :global(.narrow) { display: none; }
  }
</style>
