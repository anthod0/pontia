<script lang="ts">
  import type { Snippet } from 'svelte';
  import DevicesPage from '../../components/devices/DevicesPage.svelte';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import { initialRemoteDashboardState, isValidDeviceHandle } from '$lib/remoteDashboard';
  import { startDashboardRuntime, stopDashboardRuntime } from '../../services/dashboardRuntime';

  let { handle, children }: { handle?: string; children: Snippet } = $props();
  const publicDevBridge = import.meta.env.DEV;
  const state = $derived(handle ? (publicDevBridge && isValidDeviceHandle(handle) ? 'available' : initialRemoteDashboardState(handle)) : 'invalid');

  $effect(() => {
    if (!publicDevBridge || !handle || !isValidDeviceHandle(handle)) return;
    startDashboardRuntime();
    return stopDashboardRuntime;
  });
</script>

{#if handle}
  <RemoteDashboardPage {handle} {state}>
    {#snippet dashboard()}
      {@render children()}
    {/snippet}
  </RemoteDashboardPage>
{:else}
  <DevicesPage />
{/if}
