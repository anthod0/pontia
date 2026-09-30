<script lang="ts">
  import DashboardModeLayout from '../../src/modes/public/DashboardModeLayout.svelte';

  let { handle }: { handle: string } = $props();
</script>

<DashboardModeLayout {handle}>
  <p>Dashboard content</p>
</DashboardModeLayout>
