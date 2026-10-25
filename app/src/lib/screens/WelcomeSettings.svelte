<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Card from '../ui/Card.svelte';
  import { toast } from '../ui/toast.svelte';

  /** Settings → the selected world's welcome message: what players see when they join. */
  const MAX = 4000;
  let text = $state(store.welcome);
  let saving = $state(false);
  // A world change (or a reload) brings its message.
  $effect(() => {
    text = store.welcome;
  });
  const world = $derived(store.currentWorld);
  const changed = $derived(text.trimEnd() !== store.welcome);

  async function save() {
    saving = true;
    const ok = await store.saveWelcome(text);
    saving = false;
    if (ok) toast(t('welcome.saved'));
  }
</script>

{#if world}
  <Card class="grp" label={t('welcome.title')}>
    <h2>{t('welcome.title')}</h2>
    <p class="hint">{t('welcome.hint', { world: world.name })}</p>
    <textarea aria-label={t('welcome.title')} maxlength={MAX} rows="5" value={text} oninput={(e) => (text = e.currentTarget.value)}></textarea>
    <div class="row">
      <span class="hint">{`${text.length}/${MAX}`}</span>
      <Button size="sm" variant="primary" icon="check" loading={saving} disabled={!changed} onclick={save}>{t('welcome.save')}</Button>
    </div>
  </Card>
{/if}

<style>
  h2 { font-size: 19px; padding: 16px 0 4px; }
  textarea {
    display: block;
    width: 100%;
    margin: 10px 0 8px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: 14px/1.45 var(--f-mono, monospace);
    resize: vertical;
  }
  textarea:focus-visible { outline: 3px solid var(--primary); outline-offset: 1px; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 10px; padding-bottom: 14px; }
</style>
