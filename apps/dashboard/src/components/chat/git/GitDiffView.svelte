<script lang="ts">
  import { t } from '$lib/i18n'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile } from '../../../api/types'
  import type { GitDiffTarget } from '../../../queries/workspaces'
  import GitDiffContent from './GitDiffContent.svelte'
  import GitDiffHeader from './GitDiffHeader.svelte'
  import { gitStatusTone, countDiffLines, gitChangeGroups } from './gitChanges'

  /** The diff of one uncommitted file. */
  let { device, workspaceId, file, change, onback }: {
    device: DeviceRuntime
    workspaceId: string
    file: GitDiffTarget
    /** The file's entry in the change list, when it is still listed. */
    change: GitChangedFile | undefined
    onback: () => void
  } = $props()

  // The panel recreates this view for each file and device.
  // svelte-ignore state_referenced_locally
  const diffQuery = device.workspaceQueries.createWorkspaceGitDiffQuery(() => workspaceId, () => file)
  const group = $derived(gitChangeGroups.find(item => item.group === file.group)!)
  const diff = $derived(diffQuery.data)
  // The list's counts cover the whole file; a truncated diff holds only part of it.
  const additions = $derived(diff?.binary ? null : change ? change.additions : diff && countDiffLines(diff, 'add'))
  const deletions = $derived(diff?.binary ? null : change ? change.deletions : diff && countDiffLines(diff, 'delete'))
</script>

<GitDiffHeader path={file.path} origPath={file.orig_path} backLabel={$t('git.backToChanges')} fetching={diffQuery.isFetching}
  {additions} {deletions} {onback} onrefresh={() => diffQuery.refetch()}>
  {#snippet context()}
    {#if change}<span class={gitStatusTone(change.status)}>{change.status}</span>{/if}
    <span>{$t(group.label)}</span>
  {/snippet}
</GitDiffHeader>
<GitDiffContent path={file.path} {diff} error={diffQuery.isError ? diffQuery.error : null} onretry={() => diffQuery.refetch()} />
