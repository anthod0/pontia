<script lang="ts">
  import { t } from '$lib/i18n'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitCommitFileTarget } from '../../../queries/workspaces'
  import GitDiffContent from './GitDiffContent.svelte'
  import GitDiffHeader from './GitDiffHeader.svelte'
  import { shortSha } from './gitChanges'

  /** The diff one commit made to one file. */
  let { device, workspaceId, sha, file, onback }: {
    device: DeviceRuntime
    workspaceId: string
    sha: string
    file: GitCommitFileTarget
    onback: () => void
  } = $props()

  // The panel recreates this view for each file and device.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const diffQuery = queries.createWorkspaceGitCommitDiffQuery(() => workspaceId, () => sha, () => file)
  // The commit's file list is already loaded and counts the whole file, even when the diff is cut short.
  const commitQuery = queries.createWorkspaceGitCommitQuery(() => workspaceId, () => sha)
  const entry = $derived(commitQuery.data?.files.find(item => item.path === file.path))
  const diff = $derived(diffQuery.data)
</script>

<GitDiffHeader path={file.path} origPath={file.orig_path} backLabel={$t('git.backToCommit')} fetching={diffQuery.isFetching}
  additions={entry?.binary ? null : entry?.additions} deletions={entry?.binary ? null : entry?.deletions} {onback} onrefresh={() => diffQuery.refetch()}>
  {#snippet badge()}<Badge variant="outline" class="font-mono font-normal">{shortSha(sha)}</Badge>{/snippet}
</GitDiffHeader>
<GitDiffContent path={file.path} {diff} error={diffQuery.isError ? diffQuery.error : null} onretry={() => diffQuery.refetch()} />
