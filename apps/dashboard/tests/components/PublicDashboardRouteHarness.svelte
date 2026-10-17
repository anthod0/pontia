<script lang="ts">
  import DashboardModeLayout from '../../src/modes/public/DashboardModeLayout.svelte';

  let { data }: { data: { handle: string } } = $props();
</script>

<DashboardModeLayout handle={data.handle}>
  <input aria-label="Draft" />
</DashboardModeLayout>
