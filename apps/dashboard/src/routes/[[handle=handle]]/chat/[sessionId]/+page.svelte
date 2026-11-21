<script lang="ts">
  import { page } from '$app/state';
  import { rememberLastPage } from '$lib/lastPage';
  import { useDeviceRuntime } from '../../../../device/context';
  import SessionChatPage from '../../../../pages/SessionChatPage.svelte';

  const device = useDeviceRuntime();
  $effect(() => rememberLastPage(device.id, { kind: 'chat', id: page.params.sessionId! }));
</script>

{#key page.params.sessionId}
  <SessionChatPage routeSessionId={page.params.sessionId} />
{/key}
