<script lang="ts">
  import type { WorkspaceView } from '../../src/api/types'
  import { createWorkspacesQuery } from '../../src/queries/workspaces'

  let { onData }: { onData: (workspaces: WorkspaceView[]) => void } = $props()
  const query = createWorkspacesQuery()

  $effect(() => {
    if (query.data) onData(query.data)
  })
</script>
