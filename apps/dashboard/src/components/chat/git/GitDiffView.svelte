<script lang="ts">
  import { t } from '$lib/i18n'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile } from '../../../api/types'
  import type { GitDiffTarget } from '../../../queries/workspaces'
  import GitDiffContent from './GitDiffContent.svelte'
  import GitDiffHeader from './GitDiffHeader.svelte'
  import { gitStatusTone, countDiffLines, displayChangeGroup } from './gitChanges'

  /** The diff of one uncommitted file. */
  let { device, workspaceId, file, files, change, onback, onselect }: {
    device: DeviceRuntime
    workspaceId: string
    file: GitDiffTarget
    /** The file's entry in the change list, when it is still listed. */
    change: GitChangedFile | undefined
    files: GitChangedFile[]
    onback?: () => void
    onselect: (file: GitChangedFile) => void
  } = $props()

  // The panel recreates this view for each file and device.
  // svelte-ignore state_referenced_locally
  const diffQuery = device.workspaceQueries.createWorkspaceGitDiffQuery(() => workspaceId, () => file)
  const group = $derived(displayChangeGroup(file.group))
  const diff = $derived(diffQuery.data)
  // The list's counts cover the whole file; a truncated diff holds only part of it.
  const additions = $derived(diff?.binary ? null : change ? change.additions : diff && countDiffLines(diff, 'add'))
  const deletions = $derived(diff?.binary ? null : change ? change.deletions : diff && countDiffLines(diff, 'delete'))
  const index = $derived(files.findIndex(item => item.path === file.path && item.group === file.group))
</script>

<GitDiffHeader path={file.path} origPath={file.orig_path} backLabel={$t('git.backToChanges')} fetching={diffQuery.isFetching}
  {additions} {deletions} {onback} status={change?.status} statusTone={change ? gitStatusTone(change.status) : undefined}
  {index} total={files.length} onprevious={() => onselect(files[index - 1]!)} onnext={() => onselect(files[index + 1]!)} onrefresh={() => diffQuery.refetch()}>
  {#snippet context()}
    <span>{$t(group.label)}</span>
  {/snippet}
</GitDiffHeader>
<GitDiffContent path={file.path} {diff} error={diffQuery.isError ? diffQuery.error : null} onretry={() => diffQuery.refetch()} />
