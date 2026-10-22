<script lang="ts">
  import { ago } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Card from '../ui/Card.svelte';
  import Icon from '../ui/Icon.svelte';
  import SelectField from '../ui/SelectField.svelte';
  import SettingRow from '../ui/SettingRow.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { updates } from '../update.svelte';
  import type { UpdateChannel } from '../types';

  /** Settings → Updates (design: Update-Parts): the same check as the update icon plus the automatic options. */
  const u = updates;
  const prefs = $derived(u.prefs);
  const found = $derived(u.active && u.failed === null);
  const offline = $derived(u.stage === 'error' && u.failed === 'check');
  const result = $derived.by(() => {
    if (u.stage === 'checking') return { tone: '', icon: null, text: t('upd.checking') };
    if (found) return { tone: 'primary', icon: 'update' as const, text: t('settings.upd.available', { v: u.info?.version ?? '' }) };
    if (offline) return { tone: 'bad', icon: 'alert' as const, text: t('settings.upd.offline') };
    if (prefs.lastOk > 0) return { tone: 'ok', icon: 'check' as const, text: t('settings.upd.uptodate') };
    return { tone: '', icon: null, text: t('settings.upd.never') };
  });
  const last = $derived.by(() => {
    if (offline) return prefs.lastOk ? t('settings.upd.lastOk', { t: ago(store.now - prefs.lastOk) }) : t('settings.upd.lastNever');
    return prefs.lastChecked ? t('settings.upd.last', { t: ago(store.now - prefs.lastChecked) }) : t('settings.upd.lastNever');
  });
  const channels = $derived([
    { value: 'stable', label: t('settings.upd.stable') },
    { value: 'beta', label: t('settings.upd.beta') }
  ]);
</script>

<Card class="grp" label={t('settings.updates')}>
  <h2>{t('settings.updates')}</h2>
  <div class="row">
    <div class="l">
      <p class="t">{t('settings.upd.version', { v: store.info?.version ?? '-' })}</p>
      <p class={['res', result.tone]} role="status">
        {#if u.stage === 'checking'}<span class="spin" aria-hidden="true"></span>{:else if result.icon}<Icon name={result.icon} size={18} />{/if}
        <span>{result.text}</span>
      </p>
      <p class="hint">{last}</p>
    </div>
    {#if found}
      <Button size="sm" variant="primary" icon="update" onclick={() => (u.open = true)}>{t('settings.upd.show')}</Button>
    {:else}
      <Button size="sm" icon="refresh" disabled={u.stage === 'checking'} onclick={() => u.check()}>{offline ? t('upd.retry') : t('settings.upd.now')}</Button>
    {/if}
  </div>
  <SettingRow title={t('settings.upd.auto')} hint={t('settings.upd.autoHint')}>
    <Toggle label={t('settings.upd.auto')} hideLabel checked={prefs.autoCheck} onchange={(v) => u.setPrefs({ autoCheck: v })} />
  </SettingRow>
  <SettingRow title={t('settings.upd.channel')} hint={t('settings.upd.channelHint')}>
    <SelectField label={t('settings.upd.channel')} hideLabel value={prefs.channel} options={channels} onchange={(v) => u.setPrefs({ channel: v as UpdateChannel })} />
  </SettingRow>
</Card>

<style>
  h2 { font-size: 19px; padding: 16px 0 4px; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 14px 0; border-top: 1px solid var(--line); flex-wrap: wrap; }
  .l { flex: 1 1 260px; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .t { font-weight: 700; }
  .res { display: flex; gap: 8px; align-items: center; font-weight: 700; font-size: 14.5px; color: var(--text-2); }
  .res.ok { color: var(--ok); }
  .res.primary { color: var(--primary-ink); }
  .res.bad { color: var(--bad); }
  .spin { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--primary); animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
