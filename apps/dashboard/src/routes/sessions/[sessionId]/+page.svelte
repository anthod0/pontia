<script lang="ts">
  import { page } from '$app/state';
  import SessionDetailPage from '../../../pages/SessionDetailPage.svelte';
</script>

{#key page.params.sessionId}
  <SessionDetailPage routeSessionId={page.params.sessionId} />
{/key}
