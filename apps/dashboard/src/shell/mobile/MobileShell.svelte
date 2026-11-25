<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useDashboardHost } from '../../device/hostContext'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import MobileFrame from './MobileFrame.svelte'
  let { children }: { children: Snippet } = $props()
  const host = useDashboardHost()
  // A connecting public device already has its runtime, so the frame stays mounted when it comes online.
  const device = $derived(host.activeDevice?.runtime)
</script>

{#if device}
  {#key device}<DeviceProvider {device}><MobileFrame {device}>{@render children()}</MobileFrame></DeviceProvider>{/key}
{:else}
  <MobileFrame>{@render children()}</MobileFrame>
{/if}
