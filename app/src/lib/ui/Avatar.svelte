<script lang="ts">
  import { avatarColor, initials } from '../util';

  let { name, size = 40 }: { name: string; size?: number } = $props();
</script>

<span class="av" aria-hidden="true" style:width={`${size}px`} style:height={`${size}px`} style:background={avatarColor(name)} style:font-size={`${Math.round(size * 0.375)}px`}>{initials(name)}</span>

<style>
  .av {
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 10px;
    font: 800 15px var(--f-display);
    color: #1f1a17;
    transform: rotate(-4deg);
    box-shadow: 0 2px 0 rgba(0, 0, 0, 0.18);
  }
</style>
