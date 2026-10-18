<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { RunState } from '../types';
  import Badge from './Badge.svelte';

  let { state }: { state: RunState } = $props();
  const tones = { running: 'ok', starting: 'warn', stopping: 'warn', stopped: 'neutral', failed: 'bad' } as const;
  const label = $derived(t(`run.state.${state}`));
</script>

<span role="status" aria-label={t('status.aria', { state: label })}>
  <Badge tone={tones[state]} dot={state !== 'failed'} pulse={state === 'running' || state === 'starting'} icon={state === 'failed' ? 'alert' : undefined}>
    {label}
  </Badge>
</span>
