<script lang="ts">
  import { page } from '$app/state';
  import WorkspacePage from '../../../pages/WorkspacePage.svelte';
</script>

{#key page.params.workspaceId}
  <WorkspacePage routeWorkspaceId={page.params.workspaceId} />
{/key}
