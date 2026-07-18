<script lang="ts">
  import { page } from '$app/state';
  import SessionChatPage from '../../../pages/SessionChatPage.svelte';
</script>

{#key page.params.sessionId}
  <SessionChatPage routeSessionId={page.params.sessionId} />
{/key}
