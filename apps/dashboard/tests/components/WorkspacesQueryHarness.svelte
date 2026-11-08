<script lang="ts">
  import { testDevice } from "../deviceFixture";

  import type { WorkspaceView } from '../../src/api/types'
  const { createWorkspacesQuery } = testDevice.workspaceQueries;

  let { onData }: { onData: (workspaces: WorkspaceView[]) => void } = $props()
  const query = createWorkspacesQuery()

  $effect(() => {
    if (query.data) onData(query.data)
  })
</script>
