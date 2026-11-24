<script lang="ts">
  import type { Snippet } from 'svelte';
  import DashboardEntryPage from '../../pages/DashboardEntryPage.svelte'
  import DashboardModeLayout from '$dashboard-mode/DashboardModeLayout.svelte';

  let { data, children }: { data: { handle?: string; deviceEntry?: boolean }; children: Snippet } = $props();
</script>

<DashboardModeLayout handle={data.handle}>
  {#if data.deviceEntry}<DashboardEntryPage routeScope={'/' + data.handle} />{:else}{@render children()}{/if}
</DashboardModeLayout>
