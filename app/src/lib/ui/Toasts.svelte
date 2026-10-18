<script lang="ts">
  import Icon from './Icon.svelte';
  import { dismiss, toasts } from './toast.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts.items as it (it.id)}
    <div class="toast">
      <span class="ok"><Icon name="check" /></span>
      <span class="msg">{it.message}</span>
      {#if it.action}
        <button
          class="act"
          onclick={() => {
            it.action?.();
            dismiss(it.id);
          }}>{it.actionLabel}</button
        >
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: calc(24px + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(460px, calc(100vw - 32px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 12px 12px 16px;
    border-radius: 14px;
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--lift);
  }
  .ok { color: var(--ok); display: flex; }
  .msg { flex: 1; font-weight: 700; min-width: 0; }
  .act {
    min-height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--bg);
    font: 700 14px var(--f-body);
    cursor: pointer;
    text-decoration: underline;
  }
</style>
