<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useDashboardHost } from '../../device/hostContext'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import MobileFrame from './MobileFrame.svelte'
  let { children }: { children: Snippet } = $props()
  const host = useDashboardHost()
  const device = $derived(host.mode === 'local' || host.activeDevice?.state === 'online' ? host.activeDevice?.runtime : null)
</script>

{#if device}
  {#key device}<DeviceProvider {device}><MobileFrame {device}>{@render children()}</MobileFrame></DeviceProvider>{/key}
{:else}
  <MobileFrame>{@render children()}</MobileFrame>
{/if}
