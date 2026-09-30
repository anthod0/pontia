<script lang="ts">
  import Layout from '../../src/modes/local/DashboardModeLayout.svelte';
</script>

<Layout>
  {#snippet children()}
    <div>Dashboard route content</div>
  {/snippet}
</Layout>
