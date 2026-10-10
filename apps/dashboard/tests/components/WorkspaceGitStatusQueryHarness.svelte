<script lang="ts">
  import {
    createRefreshWorkspaceGitStatusMutation,
    createWorkspaceGitStatusQuery,
  } from '../../src/queries/workspaces'

  let { workspaceId }: { workspaceId: string } = $props()
  const query = createWorkspaceGitStatusQuery(() => workspaceId)
  const refresh = createRefreshWorkspaceGitStatusMutation()
</script>

<p data-testid="git-state">{query.data?.state ?? 'loading'}</p>
<button onclick={() => refresh.mutate(workspaceId)}>Refresh</button>
