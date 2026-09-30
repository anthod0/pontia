<script lang="ts">
  import type { Snippet } from 'svelte';
  import DashboardModeLayout from '$dashboard-mode/DashboardModeLayout.svelte';

  let { data, children }: { data: { handle?: string }; children: Snippet } = $props();
</script>

<DashboardModeLayout handle={data.handle} {children} />
