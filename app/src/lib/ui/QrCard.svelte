<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { NetAddr } from '../types';
  import { copyText } from '../util';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';
  import QrCode from './QrCode.svelte';
  import Tabs from './Tabs.svelte';
  import { toast } from './toast.svelte';

  /** "Connect from a phone": QR code plus the printed, copyable LAN address (the QR is never the only way). */
  let { addrs, port, running, phone = false }: { addrs: NetAddr[]; port: number; running: boolean; phone?: boolean } = $props();
  let picked = $state('');
  const addr = $derived(addrs.find((a) => a.ip === picked) ?? addrs[0]);

  async function copy() {
    toast((await copyText(addr.endpoint)) ? t('qr.copied') : t('qr.copyFailed'));
  }
</script>

<div class="qrcard" class:phone>
  <div class="top">
    <h2>{t('qr.title')}</h2>
    <Badge tone="info" icon="wifi">{t('qr.sameWifi')}</Badge>
  </div>
  {#if !running}
    <div class="off">
      <Icon name="wifi" size={28} />
      <p class="strong">{t('qr.startFirst')}</p>
      <p class="hint">{t('qr.startFirstHint')}</p>
    </div>
  {:else if !addr}
    <div class="off" data-testid="no-net">
      <Icon name="wifi" size={28} />
      <p class="strong">{t('qr.noNetwork')}</p>
      <p class="hint">{t('qr.noNetworkHint')}</p>
    </div>
  {:else}
    {#if addrs.length > 1}
      <Tabs mode="radiogroup" label={t('qr.network')} items={addrs.map((a) => ({ id: a.ip, label: a.ip }))} value={addr.ip} onchange={(id) => (picked = id)} />
    {/if}
    <div class="body">
      <QrCode text={addr.endpoint} size={phone ? 112 : 148} />
      <div class="info">
        <div><p class="hint strong">{t('qr.lan')}</p><p class="mono big" data-testid="addr">{addr.ip}</p></div>
        <div><p class="hint strong">{t('qr.port')}</p><p class="mono big">{port}</p></div>
        <Button size="sm" icon="copy" onclick={copy}>{t('qr.copy')}</Button>
      </div>
    </div>
    <p class="hint">{t('qr.help')}</p>
  {/if}
</div>

<style>
  .qrcard { display: flex; flex-direction: column; gap: 14px; }
  .top { display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; }
  h2 { font-size: 20px; }
  .body { display: flex; gap: 18px; align-items: center; flex-wrap: wrap; }
  .info { display: flex; flex-direction: column; gap: 10px; flex: 1; min-width: 160px; align-items: flex-start; }
  .big { font-size: 22px; font-weight: 600; overflow-wrap: anywhere; }
  .strong { font-weight: 700; }
  .off {
    padding: 28px 12px;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .off .strong { color: var(--text); }
  .phone .big { font-size: 16px; }
  .phone .body { gap: 14px; flex-wrap: nowrap; }
</style>
