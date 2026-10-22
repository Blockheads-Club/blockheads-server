<script lang="ts">
  import { tick } from 'svelte';
  import { fileSize } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';
  import DangerBadge from './DangerBadge.svelte';
  import DangerBlock from './DangerBlock.svelte';
  import ModIcon from './ModIcon.svelte';
  import { mods, modErr, sidesOf } from './mods.svelte';
  import Notice from './Notice.svelte';
  import SidesBadge from './SidesBadge.svelte';
  import { permInfo, permKey } from './perms';

  /** InstallFlow: plan → permissions → (native confirm) → installing → done | failed. Esc cancels except while installing. */
  /** While the dialog closes, the last step stays on screen. */
  let last = mods.flow!;
  const f = $derived((last = mods.flow ?? last));
  const items = $derived(f.plan!.items);
  const root = $derived(items[0]);
  const twoSteps = $derived(!!f.plan && (items.length > 1 || f.plan.missing.length > 0 || f.plan.satisfied.length > 0));
  const clientNeeded = $derived(items.some((i) => i.clientRequired));
  const blocked = $derived(f.plan!.missing.length > 0);
  const nativeItem = $derived(mods.nativeItems[0]);
  /**
   * The dialog stays open from step to step, so focus would be lost with the buttons of the last step: each new step
   * puts it on its main action (the native warning on "Don't install").
   */
  let shown = '';
  $effect(() => {
    const stage = f.stage;
    if (shown && shown !== stage) {
      void tick().then(() => {
        const box = document.querySelector('[role="dialog"]');
        const pick = box?.querySelector<HTMLElement>('[data-autofocus]') ?? box?.querySelector<HTMLElement>('footer button:not([disabled]):last-of-type');
        (pick ?? box?.querySelector<HTMLElement>('button'))?.focus();
      });
    }
    shown = stage;
  });
  let url = $state('');
  let dragging = $state(false);
  let input: HTMLInputElement | undefined = $state();

  const title = $derived.by(() => {
    switch (f.stage) {
      case 'file':
        return t('install.fileTitle');
      case 'checking':
        return t('install.checking');
      case 'plan':
        return t('install.title', { name: root.name });
      case 'perms':
        return t('install.permsTitle');
      case 'native':
        return t('install.nativeTitle', { name: nativeItem.name });
      case 'installing':
        return t('install.installing', { n: items.length });
      case 'done':
        return t('install.done', { name: root.name });
      default:
        return ['checksum', 'size', 'mismatch'].includes(f.error!.code)
          ? t('install.failedCheck')
          : ['download', 'offline', 'insecure', 'tooLarge'].includes(f.error!.code)
            ? t('install.failedDownload')
            : f.error!.code === 'invalid'
              ? t('install.failedInvalid')
              : t('install.failed');
    }
  });

  function drop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const file = e.dataTransfer?.files[0];
    if (file) void mods.pickFile(file);
  }
  function chosen() {
    const file = input?.files?.[0];
    if (file) void mods.pickFile(file);
  }
  /** Read on the failed step only. */
  const details = $derived(
    [f.error!.file, f.error!.expected && `${t('install.expected')}  ${f.error!.expected}`, f.error!.got && `${t('install.got')}  ${f.error!.got}`, f.error!.detail].filter(Boolean).join('\n')
  );
  async function copy() {
    toast((await copyText(`${modErr(f.error)}\n${details}`)) ? t('install.copied') : t('qr.copyFailed'));
  }
  const pct = $derived(f.progress ? Math.round(((f.progress.step === 'download' ? f.progress.done : f.progress.total) / Math.max(1, f.progress.total)) * 90 + (f.progress.step === 'done' ? 10 : 0)) : 0);
  const stepText = $derived(f.progress?.step === 'download' ? t('install.step.download', { done: f.progress.done + 1, total: f.progress.total }) : t('install.step.write'));
</script>

<Dialog {title} tone={f.stage === 'failed' ? 'bad' : 'plain'} icon={f.stage === 'failed' ? 'x-circle' : f.stage === 'done' ? 'check' : f.stage === 'file' ? 'zip' : undefined} onclose={() => mods.close()}>
  {#if f.stage === 'file'}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class={['drop', dragging && 'over']} ondragover={(e) => { e.preventDefault(); dragging = true; }} ondragleave={() => (dragging = false)} ondrop={drop} data-testid="drop">
      <Icon name="upload" size={28} />
      <p class="strong">{t('install.drop')}</p>
      <label class="pick">
        <Icon name="file" size={18} />{t('install.choose')}
        <input bind:this={input} type="file" accept=".bhmod,.zip" onchange={chosen} />
      </label>
    </div>
    <div class="field">
      <label for="mod-url" class="strong">{t('install.link')}</label>
      <input id="mod-url" class="inp mono" type="url" bind:value={url} placeholder="https://…/my-mod-1.0.0.bhmod" />
    </div>
    <Notice tone="warn" role="note" title={t('install.notListed')}>{t('install.notListedText')}</Notice>
  {:else if f.stage === 'checking'}
    <div class="busy" role="status"><span class="spin" aria-hidden="true"></span>{t('install.checkingText')}</div>
  {:else if f.stage === 'plan'}
    <div class="tag"><Badge tone="primary">{t('install.step1')}</Badge></div>
    {#if items.length > 1}<p class="muted">{t('install.needsMore', { name: root.name, n: items.length - 1 })}</p>{/if}
    <ul class="list">
      {#each items as i (i.id)}
        <li class="item" data-testid="plan-item">
          <ModIcon id={i.id} name={i.name} size="sm" />
          <div class="mm">
            <p class="line"><b>{i.name}</b><span class="mono hint">{i.version}</span>
              {#if i.neededBy}<Badge tone="info" icon="link">{t('install.neededBy', { name: i.neededBy })}</Badge>{/if}
              {#if i.replaces}<Badge tone="primary" icon="update">{t('install.replaces', { version: i.replaces })}</Badge>{/if}
            </p>
            <p class="line">
              <SidesBadge sides={sidesOf(i.sides.includes('server'), i.sides.includes('client'))} />
              {#if i.reviewed}<span class="rev"><Icon name="shield" size={14} />{t('mods.reviewed')}</span>{:else}<span class="ext"><Icon name="alert" size={14} />{t('mods.notListed')}</span>{/if}
              {#if i.native}<DangerBadge />{/if}
            </p>
          </div>
          {#if i.size}<span class="mono hint">{fileSize(i.size)}</span>{/if}
        </li>
      {/each}
      {#each f.plan!.satisfied as s (s.id)}
        <li class="item"><ModIcon id={s.id} name={s.id} size="sm" /><div class="mm"><p class="line"><b>{s.id}</b><span class="mono hint">{s.req}</span><Badge tone="ok" icon="check">{t('install.already', { version: s.version })}</Badge></p></div></li>
      {/each}
      {#each f.plan!.missing as m (m.id)}
        <li class="item"><ModIcon id={m.id} name={m.id} size="sm" /><div class="mm"><p class="line"><b>{m.id}</b><span class="mono hint">{m.req}</span><Badge tone="bad" icon="x-circle">{t('install.missing')}</Badge></p></div></li>
      {/each}
    </ul>
    {#if blocked}
      <Notice tone="bad" title={t('install.blocked')}>{t('install.blockedText')}</Notice>
    {:else if clientNeeded}
      <Notice tone="warn" role="note" title={t('install.clientTitle')}>{t('install.clientText')}</Notice>
    {:else}
      <Notice tone="info" role="note" title={t('install.serverOnly')}>{t('install.serverOnlyText')}</Notice>
    {/if}
    <p class="hint">{t('install.total', { size: fileSize(f.plan!.totalBytes) })}</p>
  {:else if f.stage === 'perms'}
    <div class="tag"><Badge tone="primary">{twoSteps ? t('install.step2') : t('install.stepPerms')}</Badge></div>
    <p class="muted">{t('install.permsText')}</p>
    {#each items as i (i.id)}
      {@const asked = i.permissions.filter((p) => p !== 'native')}
      <div class="card">
        <p class="strong">{i.name}</p>
        {#each asked as p (p)}
          {@const info = permInfo(p)}
          {@const params = { host: info.host ?? '', name: p }}
          <label class="prow">
            <input type="checkbox" class="chk" checked={f.grants[i.id]?.includes(p)} onchange={(e) => mods.setGrant(i.id, p, e.currentTarget.checked)} />
            <span class={['pi', info.tier === 1 && 'l1', info.tier === 2 && 'l2']}><Icon name={info.icon} /></span>
            <span class="pb"><span class="strong">{t(permKey(info, '.title'), params)}</span><span class="muted desc">{t(permKey(info, '.desc'), params)}</span></span>
          </label>
        {:else}
          <p class="hint">{t('install.noPerms')}</p>
        {/each}
        {#if i.native}<p class="hint nat"><DangerBadge variant="word" />{` ${t('install.nativeNext')}`}</p>{/if}
      </div>
    {/each}
    <p class="hint"><Icon name="info" size={14} /> {t('install.permsHint')}</p>
  {:else if f.stage === 'native'}
    <DangerBadge variant="short" />
    <DangerBlock>
      <p>{t('install.nativeIntro', { name: nativeItem.name })}</p>
      <ul><li>{t('install.native1')}</li><li>{t('install.native2')}</li><li>{t('install.native3')}</li></ul>
      <p>{nativeItem.reviewed ? t('install.nativeReviewed') : t('install.nativeNotReviewed')}</p>
    </DangerBlock>
    {#if nativeItem.clientRequired}<Notice tone="warn" role="note" title={t('install.nativeClient')}>{t('install.nativeClientText')}</Notice>{/if}
    <label class="trust"><input type="checkbox" class="chk" bind:checked={f.trust} />{t('install.trust', { name: nativeItem.name })}</label>
  {:else if f.stage === 'installing'}
    <div class="bar" role="progressbar" aria-label={t('install.progress')} aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct} aria-valuetext={`${pct}%, ${stepText}`}><span style:width={`${pct}%`}></span></div>
    <p aria-live="polite" class="busy"><span class="spin" aria-hidden="true"></span>{stepText}</p>
    <p class="hint">{t('install.keepsRunning')}</p>
  {:else if f.stage === 'done'}
    <div class="tag"><Badge tone="ok">{t('install.installed')}</Badge></div>
    {#if f.result!.restart}
      <Notice tone="info" role="status" title={t('install.restartTitle')}>{store.players.length ? t('install.restartPlayers', { n: store.players.length }) : t('install.restartText')}</Notice>
    {:else}
      <p class="muted">{f.enable ? t('install.startsWithServer') : t('install.keptOff')}</p>
    {/if}
  {:else}
    <div class="tag"><Badge tone="bad">{t('install.notInstalled')}</Badge></div>
    <p class="muted" role="alert">{modErr(f.error)} <strong>{t('install.nothing')}</strong></p>
    {#if details}<pre class="log">{details}</pre>{/if}
  {/if}

  {#snippet footer()}
    {#if f.stage === 'file'}
      <Button variant="ghost" onclick={() => mods.close()}>{t('common.cancel')}</Button>
      <Button variant="primary" disabled={!url.trim()} onclick={() => mods.pickUrl(url)}>{t('install.continue')}</Button>
    {:else if f.stage === 'checking'}
      <Button variant="ghost" onclick={() => mods.close()}>{t('common.cancel')}</Button>
    {:else if f.stage === 'plan'}
      <Button variant="ghost" onclick={() => mods.close()}>{t('common.cancel')}</Button>
      <Button variant="primary" disabled={blocked} onclick={() => (f.stage = 'perms')}>{t('install.continue')}</Button>
    {:else if f.stage === 'perms'}
      {#if twoSteps}<Button variant="ghost" onclick={() => (f.stage = 'plan')}>{t('install.back')}</Button>{:else}<Button variant="ghost" onclick={() => mods.close()}>{t('common.cancel')}</Button>{/if}
      <Button onclick={() => mods.confirm(false)}>{t('install.keepOff')}</Button>
      <Button variant="primary" icon="power" onclick={() => mods.confirm(true)}>{t('install.turnOn')}</Button>
    {:else if f.stage === 'native'}
      <span class="nf">
        <Button variant="primary" autofocus onclick={() => mods.close()}>{t('install.dontInstall')}</Button>
        <Button variant="danger" icon="octagon" disabled={!f.trust} onclick={() => mods.confirm(f.enable)}>{t('install.installNative')}</Button>
      </span>
      {#if !f.trust}<p id="native-hint" class="hint">{t('install.tickToContinue')}</p>{/if}
    {:else if f.stage === 'installing'}
      <p class="hint">{t('install.wait')}</p>
    {:else if f.stage === 'done'}
      <Button variant="ghost" onclick={() => { const id = root.id; mods.close(); mods.open(id); }}>{t('install.open', { name: root.name })}</Button>
      {#if f.result!.restart}
        <Button onclick={() => { store.restartWhenEmpty = true; mods.close(); }}>{t('mods.restartEmpty')}</Button>
        <Button variant="primary" icon="refresh" onclick={() => { mods.close(); void store.restart(); }}>{t('mods.restartNow')}</Button>
      {:else}
        <Button variant="primary" onclick={() => mods.close()}>{t('common.close')}</Button>
      {/if}
    {:else}
      <Button variant="ghost" icon="copy" onclick={copy}>{t('install.copy')}</Button>
      {#if f.origin.kind === 'local'}<Button onclick={() => mods.openFile()}>{t('install.another')}</Button>{/if}
      <Button variant="primary" icon="refresh" onclick={() => mods.retry()}>{t('install.retry')}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .tag { display: flex; }
  .drop {
    border: 2px dashed var(--line-strong);
    border-radius: 14px;
    padding: 22px 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .drop.over { border-color: var(--primary); background: var(--primary-soft); }
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 0 12px;
    border-radius: var(--r-md);
    border: 2px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    font: 700 14px var(--f-body);
    box-shadow: 0 3px 0 var(--line-strong);
    cursor: pointer;
    position: relative;
  }
  .pick:focus-within { outline: 3px solid var(--focus); outline-offset: 2px; }
  .pick input { position: absolute; inset: 0; opacity: 0; cursor: pointer; width: 100%; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .inp {
    min-height: 44px;
    padding: 0 14px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font-size: 14px;
    width: 100%;
    box-sizing: border-box;
  }
  .strong { font-weight: 700; }
  .busy { display: flex; gap: 12px; align-items: center; font-weight: 700; }
  .spin {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 3px solid var(--surface-3);
    border-top-color: var(--primary);
    animation: spin 0.9s linear infinite;
    flex: none;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .list { list-style: none; margin: 0; padding: 0; border: 2px solid var(--line); border-radius: var(--r-lg); overflow: hidden; }
  .item { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-top: 1px solid var(--line); }
  .item:first-child { border-top: 0; }
  .mm { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .rev { display: inline-flex; align-items: center; gap: 5px; font: 700 12.5px var(--f-body); color: var(--ok); }
  .ext {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px 2px 7px;
    border-radius: 7px;
    font: 700 12.5px/1.5 var(--f-body);
    background: var(--warn-soft);
    color: var(--warn);
    border: 1.5px dashed color-mix(in oklab, var(--warn), transparent 40%);
  }
  .card { border: 2px solid var(--line); border-radius: var(--r-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 4px; }
  .prow { display: flex; gap: 12px; align-items: flex-start; padding: 10px 0; border-top: 1px solid var(--line); cursor: pointer; }
  .chk { width: 20px; height: 20px; accent-color: var(--primary); margin: 8px 0 0; flex: none; }
  .pi { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; flex: none; background: var(--surface-2); color: var(--text-2); }
  .pi.l1 { background: var(--warn-soft); color: var(--warn); }
  .pi.l2 { background: var(--bad-soft); color: var(--bad); }
  .pb { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .desc { font-size: 14px; }
  .nat { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .trust { display: flex; gap: 10px; align-items: flex-start; font-weight: 700; }
  .trust .chk { margin-top: 2px; }
  .nf { display: flex; gap: 10px; flex-wrap: wrap; }
  .bar { height: 10px; border-radius: 999px; background: var(--surface-3); overflow: hidden; }
  .bar span { display: block; height: 100%; border-radius: 999px; background: var(--primary); }
  .log {
    background: var(--code-bg);
    color: var(--code-text);
    font: 12.5px/1.7 var(--f-mono);
    padding: 10px 12px;
    border-radius: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 0;
  }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
