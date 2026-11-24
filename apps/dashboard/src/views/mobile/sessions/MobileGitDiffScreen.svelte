<script lang="ts">
  import { untrack } from 'svelte'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitDiffTarget, GitCommitFileTarget } from '../../../queries/workspaces'
  import GitDiffContent from '../../../components/chat/git/GitDiffContent.svelte'
  let {
    device,
    workspaceId,
    target,
  }: {
    device: DeviceRuntime
    workspaceId: string
    target: { sha: string; file: GitCommitFileTarget } | { file: GitDiffTarget }
  } = $props()
  // Each pushed screen owns one immutable target.
  const query = untrack(() => {
    const current = target
    return 'sha' in current
      ? device.workspaceQueries.createWorkspaceGitCommitDiffQuery(
          () => workspaceId,
          () => current.sha,
          () => current.file
        )
      : device.workspaceQueries.createWorkspaceGitDiffQuery(
          () => workspaceId,
          () => current.file
        )
  })
</script>

<div class="flex h-full min-w-0 flex-col">
  <GitDiffContent
    compact
    path={target.file.path}
    diff={query.data}
    error={query.error}
    onretry={() => query.refetch()}
  />
</div>
