<script lang="ts">
  import Layout from '../../src/modes/local/DashboardModeLayout.svelte';
  import { listSessionCreationOptions } from '../../src/api/client';
</script>

<Layout>
  {#snippet children()}
    {#await listSessionCreationOptions() then options}
      <div data-testid="authenticated-options">{options.length}</div>
    {:catch}
      <div data-testid="authentication-failed"></div>
    {/await}
  {/snippet}
</Layout>
