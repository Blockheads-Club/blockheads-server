<script lang="ts">
  import { t } from '../i18n.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import DangerBadge from './DangerBadge.svelte';
  import { permInfo, permKey } from './perms';

  /** One permission a mod asks for, in plain words, with what the admin granted. */
  interface Props {
    perm: string;
    granted: boolean;
    busy?: boolean;
    /** Phone: a switch instead of the badge and button. */
    compact?: boolean;
    ongrant: () => void;
    onrevoke: () => void;
  }
  let { perm, granted, busy = false, compact = false, ongrant, onrevoke }: Props = $props();
  const info = $derived(permInfo(perm));
  const p = $derived({ host: info.host ?? '', name: perm });
  const title = $derived(t(permKey(info, '.title'), p));
</script>

<li class="prow" data-testid="perm-row">
  <span class={['pi', info.tier === 1 && 'l1', info.tier === 2 && 'l2', info.tier === 'native' && 'nat']}><Icon name={info.icon} /></span>
  <div class="pb">
    <p class="pt">{title}{#if info.tier === 'native'} <DangerBadge variant="word" />{/if}</p>
    <p class="muted desc">{t(permKey(info, '.desc'), p)}</p>
  </div>
  {#if compact}
    <Toggle checked={granted} label={t(granted ? 'mods.perm.grantedAria' : 'mods.perm.notGrantedAria', { perm: title })} hideLabel disabled={busy} onchange={(v) => (v ? ongrant() : onrevoke())} />
  {:else}
    <div class="acts">
      {#if granted}
        <Badge tone={info.tier === 'native' ? 'bad' : 'ok'} icon="check">{t('mods.perm.granted')}</Badge>
        <Button size="sm" variant={info.tier === 'native' ? 'quiet-bad' : 'default'} loading={busy} onclick={onrevoke}>{info.tier === 'native' ? t('mods.perm.revokeNative') : t('mods.perm.revoke')}</Button>
      {:else}
        <Badge tone="warn" icon="alert">{t('mods.perm.notGranted')}</Badge>
        <Button size="sm" variant="primary" loading={busy} onclick={ongrant}>{t('mods.perm.grant')}</Button>
      {/if}
    </div>
  {/if}
</li>

<style>
  .prow { display: flex; gap: 14px; align-items: flex-start; padding: 12px 0; border-top: 1px solid var(--line); flex-wrap: wrap; }
  .prow:first-child { border-top: 0; }
  .pi { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; flex: none; background: var(--surface-2); color: var(--text-2); }
  .pi.l1 { background: var(--warn-soft); color: var(--warn); }
  .pi.l2 { background: var(--bad-soft); color: var(--bad); }
  .pi.nat {
    background: repeating-linear-gradient(-45deg, var(--mod-danger) 0 6px, var(--mod-danger-stripe) 6px 12px);
    color: var(--mod-on-danger);
  }
  .pb { flex: 1 1 200px; min-width: 0; }
  .pt { font-weight: 700; display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .desc { font-size: 14px; }
  .acts { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
</style>
