<script lang="ts">
  import { bytes, percent } from '../format';
  import { t } from '../i18n.svelte';
  import { parseNotes } from '../notes';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import { toast } from '../ui/toast.svelte';
  import { updates, type RestartWhen } from '../update.svelte';
  import { clock, copyText } from '../util';

  /** The update popup (design: Server-Update): one stage at a time, the footer holds that stage's actions. */
  const u = updates;
  const stage = $derived(u.stage);
  const v = $derived(u.info?.version ?? '');
  const cur = $derived(store.info?.version ?? '');
  const p = $derived(u.progress);
  const pct = $derived(percent(u.percent));
  const online = $derived(store.active ? store.players.length : 0);
  const browser = $derived(u.info?.mode === 'browser');
  const kind = $derived(u.failed);

  const badge = $derived.by(() => {
    if (stage === 'checking') return t('upd.badge.checking');
    if (stage === 'uptodate') return t('upd.badge.uptodate');
    if (stage === 'available') return t('upd.badge.available');
    if (stage === 'ready') return t('upd.badge.ready');
    if (stage === 'done') return t('upd.badge.done');
    if (stage === 'error') return kind === 'check' ? t('upd.badge.checkFailed') : t('upd.badge.failed');
    return t('upd.badge.updating');
  });
  const tone = $derived(stage === 'error' ? 'bad' : stage === 'uptodate' || stage === 'done' ? 'ok' : '');
  const title = $derived.by(() => {
    if (stage === 'checking' || stage === 'idle') return t('upd.t.checking');
    if (stage === 'uptodate') return t('upd.t.uptodate');
    if (stage === 'available') return t('upd.t.available', { v });
    if (stage === 'downloading') return t('upd.t.downloading', { v });
    if (stage === 'ready') return t('upd.t.ready');
    if (stage === 'installing') return t('upd.t.installing', { v });
    if (stage === 'done') return t('upd.t.done');
    return kind === 'check' ? t('upd.t.checkFailed') : kind === 'download' ? t('upd.t.downloadFailed') : t('upd.t.failed');
  });
  const body = $derived.by(() => {
    if (stage === 'checking' || stage === 'idle') return t('upd.b.checking');
    if (stage === 'uptodate') return t('upd.b.uptodate');
    if (stage === 'available') return t('upd.b.available', { v: cur });
    if (stage === 'downloading') return t('upd.b.downloading');
    if (stage === 'ready') return store.active ? t('upd.b.ready') : t('upd.b.readyApp');
    if (stage === 'installing') return store.active || u.step === 0 ? t('upd.b.installing') : t('upd.b.installingApp');
    if (stage === 'done') return t('upd.b.done');
    return kind === 'check' ? t('upd.b.checkFailed') : kind === 'download' ? t('upd.b.downloadFailed') : t('upd.b.failed');
  });
  const inPlace = $derived(u.info?.mode === 'replace' || u.info?.mode === 'plugin');
  const steps = $derived([
    t('upd.step.saved'),
    t('upd.step.stopped'),
    inPlace ? t('upd.step.install', { v }) : t('upd.step.installer', { v }),
    inPlace ? t('upd.step.restart') : t('upd.step.restartApp')
  ]);
  const options: { id: RestartWhen; title: string; hint: string }[] = $derived([
    { id: 'empty', title: t('upd.when.empty'), hint: t('upd.when.emptyHint') },
    { id: 'warn', title: t('upd.when.warn'), hint: t('upd.when.warnHint', { msg: t('upd.chat.warn') }) },
    { id: 'now', title: t('upd.when.now'), hint: t('upd.when.nowHint') }
  ]);
  const mb = (n: number) => bytes(n);
  const close = () => u.close();

  async function copyLog() {
    const text = [`${t('app.title')} ${cur} → ${v}`, title, u.error].filter(Boolean).join('\n');
    toast((await copyText(text)) ? t('upd.logCopied') : t('qr.copyFailed'));
  }
</script>

<Dialog {title} onclose={close} describedby="upd-body">
  {#snippet lead()}
    <img class="appic" src="/server-icon-macos.svg" alt="" width="56" height="56" />
  {/snippet}
  {#snippet eyebrow()}<span class={['badge', tone]}>{badge}</span>{/snippet}
  <p id="upd-body" class="muted body">{body}</p>

  {#if stage === 'checking' || stage === 'idle'}
    <div class="bar" role="progressbar" aria-label={t('upd.t.checking')}><span class="ind"></span></div>
    <p class="hint" role="status">{t('upd.checkingHint')}</p>
  {:else if stage === 'uptodate'}
    <div class="box ok" role="status"><Icon name="check" /><span>{t('upd.uptodate', { v: cur })}</span></div>
  {:else if stage === 'available' && u.info}
    {@const notes = parseNotes(u.info.notes)}
    {#if u.info.prerelease}<p><span class="badge warn">{t('upd.prerelease')}</span></p>{/if}
    {#if notes.length}
      <div class="notes" role="region" aria-label={t('upd.notes')}>
        {#each notes as n, i (i)}
          {#if n.kind === 'h'}<p class="nh">{#each n.spans as s, j (j)}{s.text}{/each}</p>
          {:else}
            <p class={n.kind}>{#each n.spans as s, j (j)}{#if s.bold}<strong>{s.text}</strong>{:else if s.code}<code>{s.text}</code>{:else}{s.text}{/if}{/each}</p>
          {/if}
        {/each}
      </div>
    {/if}
    <p class="hint">
      {#if u.info.asset}{`${t('upd.size', { s: mb(u.info.asset.size) })} · `}{/if}<button class="link" onclick={() => u.openNotes()}>{t('upd.fullNotes')}</button> · {t('upd.kept')}
    </p>
    <p class="hint">{u.mobile ? t('upd.android') : browser ? t('upd.browserHint') : t('upd.noStop')}</p>
  {:else if stage === 'downloading'}
    <div class="line"><strong>{p?.stage === 'verifying' ? t('upd.verifying') : t('upd.downloadingLbl')}</strong><span class="mono">{pct}</span></div>
    <div
      class="bar"
      role="progressbar"
      aria-label={t('upd.progress')}
      aria-valuenow={u.percent}
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuetext={t('upd.progressText', { p: pct, s: p?.etaS ?? 0 })}
    ><span class="f" style:width={`${u.percent}%`}></span></div>
    {#if p && p.total > 0}
      <p class="hint mono">{t('upd.stats', { done: mb(p.done), total: mb(p.total), speed: mb(p.bytesPerS), eta: p.etaS })}</p>
    {/if}
    <p class="hint">{t('upd.keepGoing')}</p>
  {:else if stage === 'ready'}
    {#if u.scheduled}
      <div class="box info" role="status"><Icon name="history" />
        <span>{u.scheduled.kind === 'empty' ? t('upd.scheduled.empty') : t('upd.scheduled.warn', { time: clock(u.scheduled.at) })}</span>
      </div>
    {:else if online > 0}
      <div class="box warn" role="status"><Icon name="users" /><span><strong>{t('upd.online', { n: online })}</strong> {t('upd.onlineHint')}</span></div>
      <fieldset>
        <legend class="hint">{t('upd.when')}</legend>
        {#each options as o (o.id)}
          <label class={['opt', u.when === o.id && 'on']}>
            <input type="radio" name="upd-when" value={o.id} checked={u.when === o.id} onchange={() => (u.when = o.id)} />
            <span><strong>{o.title}</strong><br /><span class="hint">{o.hint}</span></span>
          </label>
        {/each}
      </fieldset>
    {/if}
  {:else if stage === 'installing'}
    <div class="bar" role="progressbar" aria-label={t('upd.installingLbl')}><span class="ind"></span></div>
    <ol class="steps">
      {#each steps as s, i (i)}
        <li class={['stp', i > u.step && 'dim']}>
          <span class={['si', i < u.step && 'ok', i === u.step && 'now']}>
            {#if i < u.step}<Icon name="check" size={16} />{:else if i === u.step}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="dot" size={16} />{/if}
          </span>
          {#if i === u.step}<strong>{s}</strong>{:else}{s}{/if}
        </li>
      {/each}
    </ol>
    <p class="hint">{u.step === 0 || inPlace ? t('upd.dontClose') : t('upd.installerHint')}</p>
  {:else if stage === 'done' && u.done}
    <div class="box ok" role="status"><Icon name="check" />
      <span>{u.done.downtime === null ? t('upd.doneApp', { v: u.done.version }) : t('upd.doneRunning', { v: u.done.version, s: u.done.downtime })}</span>
    </div>
  {:else if stage === 'error'}
    <div class="box bad" role="alert"><Icon name="alert" />
      <span>
        {#if kind === 'install'}<strong>{t('upd.failedBox')}</strong> {store.active ? t('upd.failedRunning', { v: cur }) : t('upd.failedStopped', { v: cur })}<br />{/if}{u.error}
      </span>
    </div>
  {/if}

  {#snippet footer()}
    {#if stage === 'checking' || stage === 'idle'}
      <Button variant="ghost" onclick={close}>{t('common.cancel')}</Button>
    {:else if stage === 'uptodate'}
      <Button variant="primary" onclick={close}>{t('upd.ok')}</Button>
    {:else if stage === 'available'}
      <Button variant="ghost" onclick={close}>{t('upd.later')}</Button>
      <Button variant="primary" icon={browser && !u.mobile ? 'ext' : 'download'} onclick={() => u.download()}>
        {u.mobile ? t('upd.androidInstall') : browser ? t('upd.openPage') : t('upd.download')}
      </Button>
    {:else if stage === 'downloading'}
      <Button variant="ghost" onclick={close}>{t('upd.hide')}</Button>
      <Button onclick={() => u.cancel()}>{t('upd.cancelDl')}</Button>
    {:else if stage === 'ready'}
      {#if u.scheduled}
        <Button variant="ghost" onclick={() => u.unschedule()}>{t('upd.cancelRestart')}</Button>
        <Button variant="primary" icon="refresh" onclick={() => u.install()}>{t('upd.restartNow')}</Button>
      {:else}
        <Button variant="ghost" onclick={close}>{t('upd.later')}</Button>
        <Button variant="primary" icon="refresh" onclick={() => u.schedule(online > 0 ? u.when : 'now')}>
          {online > 0 && u.when !== 'now' ? t('upd.scheduleBtn') : t('upd.restartNow')}
        </Button>
      {/if}
    {:else if stage === 'installing'}
      <Button variant="ghost" onclick={close}>{t('upd.hide')}</Button>
    {:else if stage === 'done'}
      <Button variant="ghost" icon="ext" onclick={() => u.openNotes()}>{t('upd.whatsNew')}</Button>
      <Button variant="primary" onclick={close}>{t('upd.done')}</Button>
    {:else}
      <Button variant="ghost" icon="copy" onclick={copyLog}>{t('upd.copyLog')}</Button>
      <Button variant="primary" icon="refresh" onclick={() => u.retry()}>{t('upd.retry')}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .appic { border-radius: 14px; flex: none; }
  .badge { align-self: flex-start; padding: 3px 10px; border-radius: 999px; font: 700 12.5px/1.4 var(--f-body); background: var(--primary-soft); color: var(--primary-ink); white-space: nowrap; }
  .badge.ok { background: var(--ok-soft); color: var(--ok); }
  .badge.bad { background: var(--bad-soft); color: var(--bad); }
  .badge.warn { background: var(--warn-soft); color: var(--warn); }
  .body { font-size: 15.5px; }
  .bar { height: 10px; border-radius: 999px; background: var(--surface-3); overflow: hidden; position: relative; }
  .bar .f { display: block; height: 100%; background: var(--primary); border-radius: 999px; transition: width var(--dur-move); }
  .bar .ind { position: absolute; inset: 0 auto 0 0; width: 40%; background: var(--primary); border-radius: 999px; animation: ind 1.3s ease-in-out infinite; }
  @keyframes ind { from { left: -40%; } to { left: 100%; } }
  .box { display: flex; gap: 10px; align-items: flex-start; padding: 12px 14px; border-radius: 12px; font-size: 14.5px; }
  .box :global(.ico) { margin-top: 2px; flex: none; }
  .box.ok { background: var(--ok-soft); } .box.ok :global(.ico) { color: var(--ok); }
  .box.warn { background: var(--warn-soft); } .box.warn :global(.ico) { color: var(--warn); }
  .box.bad { background: var(--bad-soft); } .box.bad :global(.ico) { color: var(--bad); }
  .box.info { background: var(--info-soft); } .box.info :global(.ico) { color: var(--info); }
  .notes { padding: 12px 14px; border-radius: 12px; background: var(--surface-2); display: flex; flex-direction: column; gap: 6px; font-size: 14.5px; max-height: 220px; overflow: auto; }
  .notes .nh { font-weight: 700; }
  .notes .li { padding-left: 18px; position: relative; }
  .notes .li::before { content: ''; position: absolute; left: 4px; top: 0.6em; width: 6px; height: 6px; border-radius: 50%; background: var(--text-3); }
  code { font-family: var(--f-mono); font-size: 0.92em; }
  .link { border: 0; padding: 0; background: none; color: var(--primary-ink); font: inherit; text-decoration: underline; cursor: pointer; }
  .line { display: flex; justify-content: space-between; gap: 12px; font-size: 14.5px; }
  .hint.mono { font-size: 12.5px; }
  fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  legend { margin-bottom: 6px; font-weight: 700; color: var(--text-2); }
  .opt { display: flex; gap: 12px; align-items: flex-start; padding: 12px 14px; border: 2px solid var(--line); border-radius: 12px; cursor: pointer; }
  .opt.on { border-color: var(--primary); background: var(--primary-soft); }
  .opt input { margin-top: 4px; accent-color: var(--primary); width: 18px; height: 18px; flex: none; }
  .opt:has(input:focus-visible) { outline: 3px solid var(--focus); outline-offset: 2px; }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  .stp { display: flex; gap: 10px; align-items: center; font-size: 14.5px; }
  .stp.dim { color: var(--text-3); }
  .si { width: 24px; height: 24px; border-radius: 7px; display: grid; place-items: center; flex: none; background: var(--surface-2); }
  .si.ok { background: var(--ok-soft); color: var(--ok); }
  .si.now { background: var(--primary-soft); }
  .spin { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--primary); animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) {
    .bar .ind, .spin { animation: none; }
    .bar .ind { width: 100%; opacity: 0.5; }
  }
</style>
