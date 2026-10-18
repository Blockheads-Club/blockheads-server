<script lang="ts">
  import { tick } from 'svelte';
  import { t } from '../i18n.svelte';
  import type { LogEntry, LogLevel } from '../types';
  import { LEVEL_TAGS, clock } from '../util';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    entries: LogEntry[];
    /** Stop following new lines. */
    paused?: boolean;
    /** The server accepts commands. */
    active: boolean;
    history: string[];
    phone?: boolean;
    onsend: (line: string) => void;
  }
  let { entries, paused = $bindable(false), active, history, phone = false, onsend }: Props = $props();

  const COMMANDS = [
    '/help', '/players', '/kick', '/ban', '/unban', '/admin', '/unadmin', '/mod', '/unmod',
    '/whitelist', '/unwhitelist', '/set-password', '/clear-blacklist', '/pvp-on', '/pvp-off', '/stop'
  ];
  type Filter = 'all' | 'chat' | 'join' | 'warn' | 'error';
  const FILTERS: { id: Filter; dot?: string }[] = [
    { id: 'all' },
    { id: 'chat', dot: '#86B9FF' },
    { id: 'join', dot: '#56C990' },
    { id: 'warn', dot: '#C98A00' },
    { id: 'error', dot: '#E2553A' }
  ];
  const QUICK = ['/players', '/help', '/kick '];

  let query = $state('');
  let shown = $state<Set<LogLevel>>(new Set());
  let match = $state(0);
  let cmd = $state('');
  let histIdx = $state(-1);
  let draft = '';
  let box: HTMLElement | undefined = $state();
  let search: HTMLInputElement | undefined = $state();
  let cmdInput: HTMLInputElement | undefined = $state();
  let atBottom = $state(true);
  let seen = $state(0);

  const visible = $derived(shown.size === 0 ? entries : entries.filter((e) => shown.has(e.level)));
  const q = $derived(query.trim().toLowerCase());
  const hits = $derived(q ? visible.filter((e) => e.text.toLowerCase().includes(q)).map((e) => e.id) : []);
  const unseen = $derived(atBottom ? 0 : Math.max(0, entries.length - seen));
  const histOpen = $derived(histIdx >= 0);

  $effect(() => {
    entries.length;
    if (!paused && atBottom) tick().then(() => box && (box.scrollTop = box.scrollHeight));
  });

  $effect(() => {
    q;
    match = 0;
  });

  function onscroll() {
    const near = !!box && box.scrollHeight - box.scrollTop - box.clientHeight < 24;
    if (atBottom && !near) seen = entries.length;
    atBottom = near;
  }

  function jump() {
    atBottom = true;
    box!.scrollTop = box!.scrollHeight;
  }

  function toggle(id: Filter) {
    const next = new Set(shown);
    if (id === 'all') next.clear();
    else if (next.has(id)) next.delete(id);
    else next.add(id);
    shown = next;
  }

  async function go(step: number) {
    if (!hits.length) return;
    match = (match + step + hits.length) % hits.length;
    await tick();
    box?.querySelector(`[data-id="${hits[match]}"]`)?.scrollIntoView({ block: 'center' });
  }

  function parts(text: string) {
    if (!q) return [{ s: text, hit: false }];
    const out: { s: string; hit: boolean }[] = [];
    const low = text.toLowerCase();
    let at = 0;
    for (let i = low.indexOf(q); i >= 0; i = low.indexOf(q, at)) {
      if (i > at) out.push({ s: text.slice(at, i), hit: false });
      out.push({ s: text.slice(i, i + q.length), hit: true });
      at = i + q.length;
    }
    if (at < text.length) out.push({ s: text.slice(at), hit: false });
    return out;
  }

  function windowKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      search?.focus();
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const line = cmd.trim();
    if (!line) return;
    onsend(line);
    cmd = '';
    histIdx = -1;
  }

  function cmdKey(e: KeyboardEvent) {
    if (e.key === 'ArrowUp' && history.length) {
      e.preventDefault();
      if (histIdx < 0) draft = cmd;
      histIdx = Math.min(histIdx + 1, history.length - 1);
      cmd = history[histIdx];
    } else if (e.key === 'ArrowDown' && histOpen) {
      e.preventDefault();
      histIdx--;
      cmd = histIdx < 0 ? draft : history[histIdx];
    } else if (e.key === 'Escape' && histOpen) {
      e.stopPropagation();
      histIdx = -1;
    } else if (e.key === 'Tab' && cmd.startsWith('/') && !cmd.includes(' ')) {
      const m = COMMANDS.filter((c) => c.startsWith(cmd.toLowerCase()));
      if (m.length) {
        e.preventDefault();
        cmd = m.length === 1 ? `${m[0]} ` : commonPrefix(m);
      }
    }
  }

  function commonPrefix(list: string[]) {
    let p = list[0];
    for (const s of list) while (!s.startsWith(p)) p = p.slice(0, -1);
    return p;
  }

  function quick(c: string) {
    cmd = c;
    cmdInput?.focus();
  }
</script>

<svelte:window onkeydown={windowKey} />

<div class="console" class:phone>
  <div class="tools">
    <div class="search">
      <label class="sr" for="log-q">{t('console.search')}</label>
      <span class="lead"><Icon name="search" /></span>
      <input id="log-q" class="inp" bind:this={search} bind:value={query} placeholder={t('console.search')} autocomplete="off" aria-describedby="log-qc" />
      {#if q}
        <span id="log-qc" class="hint count" aria-live="polite">
          {hits.length ? t('console.matchOf', { n: match + 1, total: hits.length }) : t('console.noMatch')}
        </span>
        <Button variant="ghost" size="sm" icon="up" label={t('console.prev')} onclick={() => go(-1)} disabled={!hits.length} />
        <Button variant="ghost" size="sm" icon="down" label={t('console.next')} onclick={() => go(1)} disabled={!hits.length} />
      {:else}
        <span id="log-qc" class="sr"></span>
      {/if}
    </div>
    <div class="chips" role="group" aria-label={t('console.show')}>
      {#each FILTERS as f (f.id)}
        <button class="chip" aria-pressed={f.id === 'all' ? shown.size === 0 : shown.has(f.id as LogLevel)} onclick={() => toggle(f.id)}>
          {#if f.dot}<span class="dot" style:color={f.dot}></span>{/if}
          {t(`console.filter.${f.id}`)}
        </button>
      {/each}
    </div>
  </div>

  <div class="term-wrap">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="term" role="log" aria-label={t('console.log')} aria-live="off" bind:this={box} {onscroll} tabindex="0" data-testid="log">
      {#each visible as e (e.id)}
        <div class={["ln", `l-${e.level}`, hits[match] === e.id && "cur"]} data-id={e.id}>
          <span class="t">{clock(e.at)}</span>
          <span class="lv">{LEVEL_TAGS[e.level]}</span>
          <span class="msg">{#each parts(e.text) as p}{#if p.hit}<mark>{p.s}</mark>{:else}{p.s}{/if}{/each}</span>
        </div>
      {:else}
        <p class="none">{entries.length ? t('console.noneShown') : t('console.empty')}</p>
      {/each}
    </div>
    {#if unseen > 0}
      <span class="jump"><Button size="sm" icon="down" onclick={jump}>{t('console.newLines', { n: unseen })}</Button></span>
    {/if}
  </div>

  <form class="cmdbar" onsubmit={submit}>
    {#if histOpen}
      <div class="hist" role="listbox" aria-label={t('console.history')}>
        <p class="hint hh"><Icon name="history" size={16} />{t('console.history')}</p>
        {#each history as h, i (h)}
          <div class="hi" role="option" aria-selected={i === histIdx}>
            <button type="button" tabindex="-1" onclick={() => { cmd = h; histIdx = -1; cmdInput?.focus(); }}>{h}</button>
          </div>
        {/each}
      </div>
    {/if}
    {#if phone}
      <div class="quick">
        {#each QUICK as c (c)}<button type="button" class="qk" disabled={!active} onclick={() => quick(c)}>{c.trim()}</button>{/each}
      </div>
    {/if}
    <label class="sr" for="log-cmd">{t('console.command')}</label>
    <div class="cmdin">
      <span class="prompt mono" aria-hidden="true">&gt;</span>
      <input
        id="log-cmd"
        bind:this={cmdInput}
        bind:value={cmd}
        onkeydown={cmdKey}
        disabled={!active}
        placeholder={active ? t('console.typeCommand') : t('console.stoppedHint')}
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        aria-describedby="log-cmdh"
        aria-autocomplete="list"
      />
    </div>
    <Button variant="primary" icon="send" type="submit" disabled={!active || !cmd.trim()} label={phone ? t('console.run') : undefined}>
      {#if !phone}{t('console.run')}{/if}
    </Button>
    <p id="log-cmdh" class="hint keys">
      <span class="kbd">↑</span><span class="kbd">↓</span>{t('console.keyHistory')} · <span class="kbd">Tab</span>{t('console.keyComplete')} ·
      <span class="kbd">Enter</span>{t('console.keyRun')} · <span class="kbd">Ctrl</span><span class="kbd">F</span>{t('console.keySearch')}
    </p>
  </form>
</div>

<style>
  .console { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .tools {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    padding: 12px 24px;
    border-bottom: 2px solid var(--line);
    background: var(--surface);
  }
  .search { position: relative; flex: 1 1 260px; max-width: 460px; display: flex; gap: 6px; align-items: center; }
  .lead { position: absolute; left: 12px; top: 10px; color: var(--text-3); display: flex; pointer-events: none; }
  .inp {
    flex: 1;
    min-width: 0;
    min-height: 40px;
    padding: 0 12px 0 40px;
    border: 2px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text);
    font: 15px var(--f-body);
  }
  .count { white-space: nowrap; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip {
    min-height: 40px;
    padding: 0 12px;
    border-radius: 999px;
    border: 2px solid var(--line-strong);
    background: var(--surface);
    color: var(--text-2);
    font: 700 13.5px var(--f-body);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .chip[aria-pressed='true'] { background: var(--text); color: var(--bg); border-color: var(--text); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
  .term-wrap { position: relative; flex: 1; min-height: 240px; display: flex; flex-direction: column; }
  .term {
    flex: 1;
    min-height: 0;
    background: var(--code-bg);
    color: var(--code-text);
    font: 13.5px/1.75 var(--f-mono);
    padding: 12px 0;
    overflow: auto;
  }
  .ln { display: grid; grid-template-columns: 76px 52px minmax(0, 1fr); gap: 12px; padding: 0 24px; }
  .ln:hover { background: rgba(255, 255, 255, 0.04); }
  .t { color: #8f8175; }
  .lv { font-weight: 600; }
  .msg { overflow-wrap: anywhere; white-space: pre-wrap; }
  .l-info .lv { color: #b9aa9c; }
  .l-chat .lv { color: #86b9ff; }
  .l-join .lv { color: #56c990; }
  .l-warn .lv { color: #f2c14e; }
  .l-error .lv { color: #ff8f80; }
  .l-cmd .lv { color: #ffb08c; }
  .l-error { background: rgba(255, 143, 128, 0.08); }
  .l-cmd .msg { color: #ffd2bd; }
  .cur { outline: 2px solid #ffd27a; outline-offset: -2px; }
  mark { background: #ffd27a; color: #1f1a17; border-radius: 3px; padding: 0 2px; }
  .none { padding: 24px; color: var(--code-dim); font-family: var(--f-body); }
  .jump { position: absolute; right: 24px; bottom: 16px; }
  .cmdbar {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 12px 24px 14px;
    background: var(--surface);
    border-top: 2px solid var(--line);
    flex-wrap: wrap;
    position: relative;
  }
  .cmdin {
    flex: 1;
    min-width: 200px;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 0 14px;
    border: 2px solid var(--primary);
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 0 0 4px color-mix(in oklab, var(--primary), transparent 80%);
  }
  .cmdin:has(input:disabled) { border-color: var(--line-strong); box-shadow: none; }
  .cmdin input { flex: 1; border: 0; background: transparent; color: var(--text); font: 15px var(--f-mono); outline: none; min-width: 0; }
  .prompt { color: var(--primary-ink); font-weight: 600; }
  .keys { flex-basis: 100%; display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .quick { flex-basis: 100%; display: flex; gap: 8px; overflow: hidden; }
  .qk {
    min-height: 40px;
    padding: 0 14px;
    border-radius: var(--r-md);
    border: 2px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    font: 600 13.5px var(--f-mono);
    box-shadow: var(--edge);
  }
  .hist {
    position: absolute;
    left: 24px;
    bottom: calc(100% + 6px);
    width: min(460px, calc(100% - 48px));
    background: var(--surface);
    border: 2px solid var(--line-strong);
    border-radius: 14px;
    box-shadow: var(--lift);
    padding: 6px;
    display: flex;
    flex-direction: column;
    max-height: 260px;
    overflow: auto;
  }
  .hh { padding: 6px 12px; font-weight: 700; display: flex; gap: 6px; align-items: center; }
  .hi { border-radius: 9px; }
  .hi button { width: 100%; text-align: left; min-height: 40px; padding: 0 12px; border: 0; background: transparent; color: var(--text); font: 14px var(--f-mono); border-radius: 9px; cursor: pointer; }
  .hi[aria-selected='true'] { background: var(--primary-soft); }
  .hi[aria-selected='true'] button { color: var(--primary-ink); }
  .phone .tools { padding: 8px 16px; }
  .phone .ln { padding: 0 12px; grid-template-columns: 52px 44px minmax(0, 1fr); gap: 8px; font-size: 12.5px; }
  .phone .cmdbar { padding: 10px 16px; }
  .phone .keys { display: none; }
</style>
