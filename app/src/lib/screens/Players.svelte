<script lang="ts">
  import { ago, duration } from '../format';
  import { t } from '../i18n.svelte';
  import PageHeader from '../layout/PageHeader.svelte';
  import { store } from '../store.svelte';
  import type { Player } from '../types';
  import Avatar from '../ui/Avatar.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Card from '../ui/Card.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import EmptyState from '../ui/EmptyState.svelte';
  import Field from '../ui/Field.svelte';
  import Menu from '../ui/Menu.svelte';
  import Table from '../ui/Table.svelte';
  import Tabs from '../ui/Tabs.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText, shortId } from '../util';
  import { viewport } from '../viewport.svelte';

  type View = 'online' | 'known';
  let view = $state<View>('online');
  let query = $state('');
  let all = $state(false);
  let kicking = $state<Player | null>(null);
  let banning = $state<Player | null>(null);
  let alsoIp = $state(true);

  const q = $derived(query.trim().toLowerCase());
  const known = $derived(store.known.filter((k) => k.name.toLowerCase().includes(q)));
  const shownKnown = $derived(all ? known : known.slice(0, 12));
  const max = $derived(store.settings.run.maxPlayers);

  function items(p: Player) {
    const role = store.roleOf(p.name);
    return [
      ...(role === 'admin'
        ? [{ id: 'unadmin', label: t('players.removeAdmin'), icon: 'crown' as const }]
        : [{ id: 'admin', label: t('players.makeAdmin'), icon: 'crown' as const }]),
      ...(role === 'mod'
        ? [{ id: 'unmod', label: t('players.removeMod'), icon: 'shield' as const }]
        : role === 'admin'
          ? []
          : [{ id: 'mod', label: t('players.makeMod'), icon: 'shield' as const }]),
      { id: 'copy', label: t('players.copyId'), icon: 'copy' as const },
      { id: 'kick', label: t('players.kickAsk'), icon: 'logout' as const, tone: 'bad' as const, separated: true },
      { id: 'ban', label: t('players.banAsk'), icon: 'ban' as const, tone: 'bad' as const }
    ];
  }

  async function act(p: Player, id: string) {
    if (id === 'admin' || id === 'mod') {
      if (await store.addName(id, p.name)) toast(t(`players.nowRole.${id}`, { name: p.name }), { label: t('common.undo'), run: () => store.removeName(id, p.name) });
    } else if (id === 'unadmin' || id === 'unmod') {
      await store.removeName(id === 'unadmin' ? 'admin' : 'mod', p.name);
    } else if (id === 'copy') {
      toast((await copyText(p.id)) ? t('players.idCopied') : t('qr.copyFailed'));
    } else if (id === 'kick') {
      kicking = p;
    } else {
      alsoIp = true;
      banning = p;
    }
  }

  async function doKick() {
    const p = kicking!;
    kicking = null;
    await store.kick(p.name);
  }

  async function doBan() {
    const p = banning!;
    banning = null;
    await store.ban(p.name, alsoIp ? p.ip : undefined);
  }
</script>

{#snippet roleBadge(name: string)}
  {#if store.roleOf(name) === 'admin'}<Badge tone="primary" icon="crown">{t('role.admin')}</Badge>
  {:else if store.roleOf(name) === 'mod'}<Badge tone="info" icon="shield">{t('role.mod')}</Badge>{/if}
{/snippet}

<PageHeader title={t('nav.players')} subtitle={max > 0 ? t('players.ofMax', { n: store.players.length, max }) : t('players.onlineCount', { n: store.players.length })}>
  {#snippet actions()}
    <Tabs
      label={t('players.lists')}
      value={view}
      onchange={(v) => (view = v)}
      items={[
        { id: 'online', label: t('players.tabOnline', { n: store.players.length }), dot: 'var(--ok)' },
        { id: 'known', label: t('players.tabKnown', { n: store.known.length }) }
      ]}
    />
  {/snippet}
</PageHeader>

<div class="main">
  {#if view === 'online'}
    {#if store.players.length === 0}
      <Card>
        <EmptyState icon="users" title={t('players.emptyTitle')} text={store.state === 'running' ? t('players.emptyRunning') : t('players.emptyStopped')}>
          {#if store.state !== 'running'}<Button variant="primary" icon="play" onclick={() => store.navigate('dashboard')}>{t('players.goDashboard')}</Button>{/if}
        </EmptyState>
      </Card>
    {:else}
      <Table
        label={t('players.online')}
        template="52px minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr) auto"
        columns={[
          { key: 'a', label: t('players.colAvatar'), srOnly: true },
          { key: 'p', label: t('players.colPlayer') },
          { key: 'r', label: t('players.colRole'), narrow: true },
          { key: 'o', label: t('players.colOnline'), narrow: true },
          { key: 'x', label: t('players.colActions'), align: 'end' }
        ]}
      >
        {#snippet rows()}
          {#each store.players as p (p.id)}
            <div class="tr" role="row" data-testid="player">
              <span role="cell"><Avatar name={p.name} /></span>
              <span role="cell" class="who">
                <span class="mono name" title={p.name}>{p.name}</span>
                <span class="hint mono" title={p.ip}>{t('players.device', { id: shortId(p.id) })}</span>
              </span>
              <span role="cell" class="narrow">{#if store.roleOf(p.name)}{@render roleBadge(p.name)}{:else}<span class="hint">{t('players.rolePlayer')}</span>{/if}</span>
              <span role="cell" class="narrow">{duration(store.now - p.joinedAt)}</span>
              <span role="cell" class="acts"><Menu label={t('players.actionsFor', { name: p.name })} items={items(p)} onselect={(id) => act(p, id)} /></span>
            </div>
          {/each}
        {/snippet}
      </Table>
    {/if}
  {:else}
    <Card>
      <div class="kh">
        <h2>{t('players.knownTitle')}</h2>
        <div class="ks"><Field label={t('players.searchKnown')} hideLabel icon="search" type="search" placeholder={t('players.searchPh')} bind:value={query} /></div>
      </div>
      <p class="hint">{t('players.knownNote')}</p>
      {#if known.length === 0}
        <p class="muted" data-testid="no-known">{store.known.length ? t('players.noMatch', { q: query.trim() }) : t('players.knownEmpty')}</p>
      {:else}
        <div class="grid">
          {#each shownKnown as k (k.name)}
            {@const online = store.players.some((p) => p.name === k.name)}
            <div class="kp" data-testid="known">
              <Avatar name={k.name} size={34} />
              <div class="who">
                <p class="mono kn" title={k.name}>{k.name}</p>
                <p class="hint">{online ? t('players.onlineNow') : t('players.lastSeen', { ago: ago(store.now - k.lastSeen) })}</p>
              </div>
              {#if store.has('black', k.name)}<Badge tone="bad">{t('players.banned')}</Badge>
              {:else if store.roleOf(k.name)}{@render roleBadge(k.name)}
              {:else if store.has('white', k.name)}<Badge>{t('players.whitelisted')}</Badge>{/if}
            </div>
          {/each}
        </div>
        {#if known.length > 12}
          <button class="more" onclick={() => (all = !all)}>{all ? t('chips.less') : t('players.showAll', { n: known.length })}</button>
        {/if}
      {/if}
    </Card>
  {/if}
</div>

{#if kicking}
  <Dialog title={t('players.kickTitle', { name: kicking.name })} icon="logout" tone="bad" description={t('players.kickText')} onclose={() => (kicking = null)}>
    {#snippet footer()}
      <Button onclick={() => (kicking = null)}>{t('common.cancel')}</Button>
      <Button variant="danger" icon="logout" onclick={doKick}>{t('players.kickBtn')}</Button>
    {/snippet}
  </Dialog>
{/if}

{#if banning}
  <Dialog title={t('players.banTitle', { name: banning.name })} icon="ban" tone="bad" description={t('players.banText')} onclose={() => (banning = null)}>
    <label class="chk"><input type="checkbox" bind:checked={alsoIp} />{t('players.alsoIp', { ip: banning.ip })}</label>
    {#snippet footer()}
      <Button onclick={() => (banning = null)}>{t('common.cancel')}</Button>
      <Button variant="danger" icon="ban" onclick={doBan}>{t('players.banBtn')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .main { padding: 20px 28px; display: flex; flex-direction: column; gap: 18px; }
  .who { min-width: 0; display: flex; flex-direction: column; }
  .name, .kn { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kn { font-size: 14.5px; }
  .acts { display: flex; justify-content: flex-end; }
  .kh { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; margin-bottom: 6px; }
  h2 { font-size: 19px; }
  .ks { width: min(300px, 100%); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(240px, 100%), 1fr)); gap: 10px; margin-top: 12px; }
  .kp { display: flex; gap: 12px; align-items: center; padding: 10px 12px; border-radius: 12px; background: var(--surface-2); }
  .kp .who { flex: 1; }
  .more { border: 0; background: none; color: var(--primary-ink); font: 700 14.5px var(--f-body); min-height: 44px; cursor: pointer; text-decoration: underline; }
  .chk { display: flex; gap: 10px; align-items: center; font-size: 15px; min-height: 44px; }
  .chk input { width: 20px; height: 20px; accent-color: var(--primary); margin: 0; }
  @media (max-width: 599px) {
    .main { padding: 6px 16px 20px; }
  }
</style>
