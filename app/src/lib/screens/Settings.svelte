<script lang="ts">
  import { LANGUAGES, i18n, t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import { setTheme, theme } from '../theme.svelte';
  import type { ThemeChoice } from '../types';
  import Button from '../ui/Button.svelte';
  import Card from '../ui/Card.svelte';
  import Field from '../ui/Field.svelte';
  import SelectField from '../ui/SelectField.svelte';
  import SettingRow from '../ui/SettingRow.svelte';
  import Tabs from '../ui/Tabs.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';

  const run = $derived(store.settings.run);
  let port = $state(String(store.settings.run.port));
  let max = $state(String(store.settings.run.maxPlayers));
  let pw = $state(store.settings.run.password);
  let delay = $state(String(store.settings.run.saveDelay));
  let showPw = $state(false);
  let saving = $state(false);

  const int = (s: string) => (/^\d+$/.test(s.trim()) ? Number(s) : NaN);
  const portErr = $derived(int(port) >= 1024 && int(port) <= 65535 ? '' : t('settings.portErr'));
  const maxErr = $derived(int(max) >= 0 && int(max) <= 64 ? '' : t('settings.maxErr'));
  const delayErr = $derived(int(delay) >= 1 && int(delay) <= 3600 ? '' : t('settings.delayErr'));
  const pwErr = $derived(pw.length < 16 ? '' : t('settings.pwErr'));
  const valid = $derived(!portErr && !maxErr && !delayErr && !pwErr);
  const changes = $derived(
    [int(port) !== run.port, int(max) !== run.maxPlayers, pw !== run.password, int(delay) !== run.saveDelay].filter(Boolean).length
  );
  const mobile = $derived(store.info?.platform === 'android');

  function discard() {
    port = String(run.port);
    max = String(run.maxPlayers);
    pw = run.password;
    delay = String(run.saveDelay);
  }

  async function save() {
    saving = true;
    const ok = await store.applyRun({ ...run, port: int(port), maxPlayers: int(max), password: pw, saveDelay: int(delay) });
    saving = false;
    if (ok) toast(t('settings.saved'));
  }

  async function copyDir() {
    toast((await copyText(store.info?.dataDir ?? '')) ? t('settings.dirCopied') : t('qr.copyFailed'));
  }

  const themes = $derived([
    { id: 'light' as const, label: t('settings.themeLight'), icon: 'sun' as const },
    { id: 'dark' as const, label: t('settings.themeDark'), icon: 'moon' as const },
    { id: 'system' as const, label: t('settings.themeSystem'), icon: 'monitor' as const }
  ]);
</script>

<PageHeader title={t('settings.title')} subtitle={t('settings.subtitle')} />

<div class="main">
  <Card class="grp" label={t('settings.connection')}>
    <h2>{t('settings.connection')}</h2>
    <SettingRow title={t('settings.port')} hint={t('settings.portHint')} restart>
      <Field label={t('settings.port')} hideLabel mono center inputmode="numeric" width="120px" bind:value={port} error={portErr} />
    </SettingRow>
    <SettingRow title={t('settings.maxPlayers')} hint={t('settings.maxHint')} restart titleId="mpl">
      <input
        class="range"
        type="range"
        min="0"
        max="64"
        value={Number.isNaN(int(max)) ? 0 : Math.min(int(max), 64)}
        oninput={(e) => (max = e.currentTarget.value)}
        aria-labelledby="mpl"
        aria-valuetext={t('settings.playersValue', { n: max })}
      />
      <Field label={t('settings.maxPlayers')} hideLabel mono center inputmode="numeric" width="84px" bind:value={max} error={maxErr} />
    </SettingRow>
    <SettingRow title={t('settings.password')} hint={t('settings.passwordHint')} restart>
      <Field label={t('settings.password')} hideLabel type={showPw ? 'text' : 'password'} maxlength={20} width="220px" bind:value={pw} error={pwErr}>
        {#snippet trailing()}
          <Button variant="ghost" size="sm" icon="eye" label={showPw ? t('settings.hidePw') : t('settings.showPw')} pressed={showPw} onclick={() => (showPw = !showPw)} />
        {/snippet}
      </Field>
    </SettingRow>
  </Card>

  <Card class="grp" label={t('settings.saving')}>
    <h2>{t('settings.saving')}</h2>
    <SettingRow title={t('settings.saveDelay')} hint={t('settings.saveDelayHint')} restart>
      <Field label={t('settings.saveDelay')} hideLabel mono center inputmode="numeric" width="96px" bind:value={delay} error={delayErr} />
      <span class="muted">{t('settings.seconds')}</span>
    </SettingRow>
    <SettingRow title={t('settings.dataDir')} hint={store.info?.dataDir ?? '-'} mono>
      <Button size="sm" icon="copy" onclick={copyDir}>{t('settings.copyPath')}</Button>
    </SettingRow>
  </Card>

  {#if !mobile}
    <Card class="grp" label={t('settings.computer')}>
      <h2>{t('settings.computer')}</h2>
      <SettingRow title={t('settings.boot')} hint={t('settings.bootHint')}>
        <Toggle label={t('settings.boot')} hideLabel disabled /><span class="hint">{t('common.soon')}</span>
      </SettingRow>
      <SettingRow title={t('settings.tray')} hint={t('settings.trayHint')}>
        <Toggle label={t('settings.tray')} hideLabel disabled /><span class="hint">{t('common.soon')}</span>
      </SettingRow>
      <SettingRow title={t('settings.web')} hint={t('settings.webHint')}>
        <Toggle label={t('settings.web')} hideLabel disabled /><span class="hint">{t('common.soon')}</span>
      </SettingRow>
    </Card>
  {/if}

  <Card class="grp" label={t('settings.appearance')}>
    <h2>{t('settings.appearance')}</h2>
    <SettingRow title={t('settings.language')}>
      <SelectField label={t('settings.language')} hideLabel value={i18n.lang} options={LANGUAGES.map((l) => ({ value: l.code, label: l.label, lang: l.code }))} onchange={(v) => store.setLanguage(v)} />
    </SettingRow>
    <SettingRow title={t('settings.theme')}>
      <Tabs mode="radiogroup" label={t('settings.theme')} items={themes} value={theme.choice} onchange={(c: ThemeChoice) => setTheme(c)} />
    </SettingRow>
  </Card>

  <Card class="grp" label={t('settings.about')}>
    <h2>{t('settings.about')}</h2>
    <div class="about">
      <img src="/server-icon-macos.svg" alt="" width="44" height="44" />
      <div class="ab">
        <p class="t">{t('app.title')}</p>
        <p class="hint">{t('settings.version')} <span class="mono">{store.info?.version ?? '-'}</span> · {t('settings.fan')}</p>
        <p class="hint">{t('settings.license')}</p>
      </div>
      <Button size="sm" icon="code" href="https://github.com/Blockheads-Club/blockheads-server">GitHub</Button>
    </div>
  </Card>
</div>

{#if changes > 0}
  <div class="savebar" role="region" aria-label={t('settings.unsaved')}>
    <p><span class="dotw"></span>{t('settings.changes', { n: changes })}{store.active ? ` · ${t('settings.needsRestart')}` : ''}</p>
    <div class="bar-btns">
      <button class="discard" onclick={discard}>{t('settings.discard')}</button>
      <Button size="sm" variant="primary" icon={store.active ? 'refresh' : 'check'} loading={saving} disabled={!valid} onclick={save}>
        {store.active ? t('settings.saveRestart') : t('settings.save')}
      </Button>
    </div>
  </div>
{/if}

<style>
  .main { padding: 24px 28px 120px; display: flex; flex-direction: column; gap: 18px; max-width: 900px; }
  .main :global(.grp) { padding: 4px 22px; }
  h2 { font-size: 19px; padding: 16px 0 4px; }
  .range { accent-color: var(--primary); width: 200px; height: 28px; }
  .about { display: flex; gap: 14px; align-items: center; flex-wrap: wrap; padding: 14px 0; border-top: 1px solid var(--line); }
  .about img { border-radius: 10px; }
  .ab { flex: 1 1 240px; }
  .t { font-weight: 700; }
  .savebar {
    position: sticky;
    bottom: 16px;
    margin: 0 28px 16px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    padding: 12px 14px 12px 18px;
    border-radius: 16px;
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--lift);
    max-width: 900px;
  }
  .savebar p { font-weight: 700; display: flex; gap: 10px; align-items: center; }
  .dotw { width: 10px; height: 10px; border-radius: 50%; background: var(--warn); display: inline-block; }
  .bar-btns { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .discard { background: transparent; border: 0; color: var(--bg); font: 700 14px var(--f-body); min-height: 40px; padding: 0 12px; cursor: pointer; text-decoration: underline; }
  @media (max-width: 599px) {
    .main { padding: 6px 16px 120px; }
    .savebar { margin: 0 12px 12px; bottom: 76px; }
  }
</style>
