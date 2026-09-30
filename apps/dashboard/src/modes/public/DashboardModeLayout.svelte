<script lang="ts">
  import type { Snippet } from 'svelte';
  import DevicesPage from '../../components/devices/DevicesPage.svelte';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import { initialRemoteDashboardState } from '$lib/remoteDashboard';

  let { handle, children }: { handle?: string; children: Snippet } = $props();
  const state = $derived(handle ? initialRemoteDashboardState(handle) : 'invalid');
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
