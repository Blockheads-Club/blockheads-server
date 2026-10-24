<script lang="ts">
  import { t } from '../i18n.svelte';
  import { DEFAULT_MODS, store } from '../store.svelte';
  import Alert from '../ui/Alert.svelte';
  import Card from '../ui/Card.svelte';
  import Field from '../ui/Field.svelte';
  import SettingRow from '../ui/SettingRow.svelte';
  import Toggle from '../ui/Toggle.svelte';

  /** Settings → Mods: the switch for JVM (Java/Kotlin) mods. Off by default; they cannot be sandboxed. */
  const prefs = $derived(store.settings.mods ?? DEFAULT_MODS);
  const built = $derived(store.info?.jvmBuilt ?? false);

  function set(change: Partial<typeof DEFAULT_MODS>) {
    store.settings.mods = { ...prefs, ...change };
    return store.saveSettings();
  }
</script>

<Card class="grp" label={t('settings.jvm.title')}>
  <h2>{t('settings.jvm.title')}</h2>
  <SettingRow title={t('settings.jvm.switch')} hint={t('settings.jvm.hint')} restart>
    <Toggle label={t('settings.jvm.switch')} hideLabel checked={prefs.jvm} onchange={(v) => set({ jvm: v })} />
  </SettingRow>
  <Alert tone="bad" title={t('settings.jvm.warnTitle')}>{t('settings.jvm.warn')}</Alert>
  {#if prefs.jvm}
    <SettingRow title={t('settings.jvm.home')} hint={t('settings.jvm.homeHint')} restart>
      <Field label={t('settings.jvm.home')} hideLabel mono width="280px" placeholder="/usr/lib/jvm/…" value={prefs.javaHome} oninput={(v) => set({ javaHome: v })} />
    </SettingRow>
  {/if}
  {#if !built}<p class="hint note">{t('settings.jvm.notBuilt')}</p>{/if}
</Card>

<style>
  h2 { font-size: 19px; padding: 16px 0 4px; }
  .note { padding: 0 0 14px; }
</style>
