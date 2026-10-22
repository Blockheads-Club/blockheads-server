<script lang="ts">
  import { api } from '../api';
  import { ago, duration } from '../format';
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { LogEntry, NetAddr } from '../types';
  import Avatar from '../ui/Avatar.svelte';
  import Badge from '../ui/Badge.svelte';
  import Card from '../ui/Card.svelte';
  import ModsCard from '../mods/ModsCard.svelte';
  import Icon from '../ui/Icon.svelte';
  import PerfCard from '../ui/PerfCard.svelte';
  import QrCard from '../ui/QrCard.svelte';
  import StatusBanners from '../ui/StatusBanners.svelte';
  import StatusHero from '../ui/StatusHero.svelte';
  import { clock, shortId } from '../util';
  import { viewport } from '../viewport.svelte';

  let addrs = $state<NetAddr[]>([]);
  const run = $derived(store.settings.run);
  const latest = $derived(store.log.slice(-3));
  const statusColor = $derived(
    { running: 'var(--ok)', starting: 'var(--warn)', stopping: 'var(--warn)', failed: 'var(--bad)', stopped: 'var(--text-2)' }[store.state]
  );

  const line = (e: LogEntry) => `${clock(e.at)}  ${e.text}`;

  $effect(() => {
    const port = run.port;
    api.networkInfo(port).then((a) => (addrs = a), () => (addrs = []));
  });
</script>

{#snippet hero()}
  <StatusHero
    state={store.state}
    worldName={store.currentWorld?.name}
    phone={viewport.phone}
    onstart={() => store.start()}
    onstop={() => store.stop()}
    onrestart={() => store.restart()}
  />
{/snippet}

<PageHeader
  title={viewport.phone ? t('app.server') : t('nav.dashboard')}
  subtitle={store.currentWorld ? t('dash.worldIs', { name: store.currentWorld.name }) : t('dash.noWorld')}
  actions={viewport.phone ? undefined : hero}
/>

<div class="main">
  {#if viewport.phone}{@render hero()}{/if}
  <StatusBanners
    state={store.state}
    worldName={store.currentWorld?.name}
    detail={store.detail}
    offline={store.offline}
    onretry={() => store.pollOnce()}
    onport={() => store.navigate('settings')}
    onlog={() => store.navigate('console')}
  />

  <div class="tiles">
    <Card pad={false} class="tile"><span class="k">{t('dash.status')}</span><span class="v" style:color={statusColor} data-testid="state">{t(`run.state.${store.state}`)}</span></Card>
    <Card pad={false} class="tile">
      <span class="k">{t('dash.players')}</span>
      <span class="v">{store.players.length}{#if run.maxPlayers > 0}<span class="muted of">{` / ${run.maxPlayers}`}</span>{/if}</span>
    </Card>
    <Card pad={false} class="tile">
      <span class="k">{t('dash.uptime')}</span>
      <span class="v">{store.uptimeMs === null ? '—' : duration(store.uptimeMs)}</span>
    </Card>
    <Card pad={false} class="tile">
      <span class="k">{t('dash.world')}</span>
      <span class="v world">{store.currentWorld?.name ?? '—'}</span>
      {#if store.currentWorld}<span class="hint mono">{shortId(store.currentWorld.id)}</span>{/if}
    </Card>
  </div>

  <ModsCard />

  <div class="two">
    <Card class="col"><QrCard {addrs} port={run.port} running={store.state === 'running'} phone={viewport.phone} /></Card>

    <Card class="col">
      <div class="ph">
        <h2>{t('dash.onlineTitle')}</h2>
        <button class="link" onclick={() => store.navigate('players')}>{t('dash.allPlayers')}<Icon name="chev-r" size={18} /></button>
      </div>
      {#if store.players.length === 0}
        <p class="muted none" data-testid="no-players">{t('dash.nobody')}</p>
      {:else}
        {#each store.players.slice(0, 5) as p (p.id)}
          <div class="prow" data-testid="player">
            <Avatar name={p.name} size={36} />
            <div class="who">
              <p class="mono name" title={p.name}>{p.name}</p>
              <p class="hint">{t('dash.joined', { ago: ago(store.now - p.joinedAt) })}</p>
            </div>
            {#if store.roleOf(p.name) === 'admin'}<Badge tone="primary" icon="crown">{t('role.admin')}</Badge>
            {:else if store.roleOf(p.name) === 'mod'}<Badge tone="info" icon="shield">{t('role.mod')}</Badge>{/if}
          </div>
        {/each}
      {/if}
      <div class="latest">
        <p class="hint strong">{t('dash.latest')}</p>
        <pre class="log" role="log" aria-label={t('dash.latest')}>{#each latest as e (e.id)}{line(e)}
{:else}{t('console.empty')}{/each}</pre>
      </div>
    </Card>
  </div>
  <Card><PerfCard stats={store.stats} history={store.statsHistory} running={store.state === 'running'} /></Card>
</div>

<style>
  .main { padding: 24px 28px; display: flex; flex-direction: column; gap: 20px; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(180px, 100%), 1fr)); gap: 14px; }
  .tiles :global(.tile) { padding: 16px 18px; display: flex; flex-direction: column; gap: 6px; }
  .k { font-size: 13.5px; font-weight: 700; color: var(--text-3); }
  .v { font: 800 30px/1.05 var(--f-display); letter-spacing: -0.01em; overflow-wrap: anywhere; }
  .v.world { font-size: 23px; line-height: 1.25; }
  .of { font-size: 18px; }
  .two { display: flex; flex-wrap: wrap; gap: 20px; align-items: stretch; }
  .two :global(.col) { flex: 1 1 340px; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .ph { display: flex; justify-content: space-between; align-items: center; gap: 10px; margin-bottom: 6px; }
  h2 { font-size: 20px; }
  .link {
    border: 0;
    background: none;
    color: var(--primary-ink);
    font: 700 14.5px var(--f-body);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 44px;
    cursor: pointer;
  }
  .none { padding: 20px 0; }
  .prow { display: flex; gap: 12px; align-items: center; padding: 10px 0; border-bottom: 1px solid var(--line); }
  .who { flex: 1; min-width: 0; }
  .name { font-weight: 600; overflow: hidden; text-overflow: ellipsis; }
  .latest { margin-top: auto; padding-top: 10px; }
  .strong { font-weight: 700; margin-bottom: 6px; }
  .log {
    background: var(--code-bg);
    color: var(--code-text);
    font: 13px/1.8 var(--f-mono);
    padding: 12px 14px;
    border-radius: 12px;
    margin: 0;
    white-space: pre-wrap;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  @media (max-width: 599px) {
    .main { padding: 6px 16px 20px; gap: 12px; }
    .v { font-size: 24px; }
    .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
    .tiles :global(.tile) { padding: 12px 14px; }
    .two { flex-direction: column; flex-wrap: nowrap; }
    .two :global(.col) { flex: none; }
  }
</style>
