<script lang="ts">
  import { page } from '$app/state'
  import WorkflowDetailPage from '../../../pages/WorkflowDetailPage.svelte'
</script>

{#key page.params.workflowId}
  <WorkflowDetailPage routeWorkflowId={page.params.workflowId ?? ''} />
{/key}
