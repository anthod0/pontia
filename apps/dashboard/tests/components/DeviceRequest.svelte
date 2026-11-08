<script lang="ts">
  import { useDeviceRuntime } from '../../src/device/context';
  const { listSessionCreationOptions } = useDeviceRuntime().api;
</script>

{#await listSessionCreationOptions() then options}
  <div data-testid="authenticated-options">{options.length}</div>
{:catch}
  <div data-testid="authentication-failed"></div>
{/await}
