<script lang="ts">
  import InstallFlow from '../mods/InstallFlow.svelte';
  import ModDetail from '../mods/ModDetail.svelte';
  import ModsBrowse from '../mods/ModsBrowse.svelte';
  import ModsList from '../mods/ModsList.svelte';
  import { mods } from '../mods/mods.svelte';
</script>

{#if mods.view === 'browse'}
  <ModsBrowse />
{:else if mods.view === 'detail' && mods.detail}
  <ModDetail mod={mods.detail} />
{:else}
  <ModsList />
{/if}
{#if mods.flow}<InstallFlow />{/if}
