<script lang="ts">
  import QRCode from 'qrcode';

  let { text, size = 148 }: { text: string; size?: number } = $props();
  let svg = $state('');

  $effect(() => {
    let alive = true;
    QRCode.toString(text, { type: 'svg', margin: 0, width: size - 24 }).then((s) => {
      if (alive) svg = s;
    });
    return () => (alive = false);
  });
</script>

<div class="qr" role="img" aria-label={text} data-testid="qr" style:width={`${size}px`} style:height={`${size}px`}>{@html svg}</div>

<style>
  .qr {
    background: #fff;
    border-radius: 12px;
    padding: 10px;
    border: 2px solid var(--line-strong);
    line-height: 0;
    flex: none;
  }
  .qr :global(svg) { width: 100%; height: 100%; }
</style>
