<script lang="ts">
  import { untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import TextAlignLeftIcon from 'phosphor-svelte/lib/TextAlignLeftIcon'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile, GitCommitFile } from '../../../api/types'
  import type { GitDiffTarget, GitCommitFileTarget } from '../../../queries/workspaces'
  import { Button } from '$lib/components/ui/button/index.js'
  import GitDiffContent from '../../../components/chat/git/GitDiffContent.svelte'

  let { device, workspaceId, target, files }: {
    device: DeviceRuntime
    workspaceId: string
    target: { sha: string; file: GitCommitFileTarget } | { file: GitDiffTarget }
    files: (GitChangedFile | GitCommitFile)[]
  } = $props()
  const initialTarget = untrack(() => target)
  const initialFiles = untrack(() => files)
  let index = $state(initialFiles.findIndex(file => file.path === initialTarget.file.path))
  let wrap = $state(true)
  const file = $derived(initialFiles[Math.max(0, index)] ?? initialTarget.file)
  // Each pushed screen owns one device and immutable target.
  // svelte-ignore state_referenced_locally
  const query = 'sha' in initialTarget
    ? device.workspaceQueries.createWorkspaceGitCommitDiffQuery(
        () => workspaceId,
        () => initialTarget.sha,
        () => ({ path: file.path, orig_path: file.orig_path }),
      )
    : device.workspaceQueries.createWorkspaceGitDiffQuery(
        () => workspaceId,
        () => ({ path: file.path, orig_path: file.orig_path, group: (file as GitChangedFile).group }),
      )
</script>

<div class="flex h-full min-w-0 flex-col">
  <GitDiffContent compact {wrap} path={file.path} diff={query.data} error={query.error} onretry={() => query.refetch()} />
  <div class="mx-auto mb-3 flex h-10 shrink-0 items-center gap-1 rounded-full border bg-card/90 px-1 shadow-lg backdrop-blur-md">
    <Button variant="ghost" size="icon-sm" class="rounded-full" disabled={index <= 0} onclick={() => { index -= 1 }} aria-label={$t('git.previousFile')}><CaretLeftIcon class="size-4" /></Button>
    <span class="min-w-12 text-center text-xs tabular-nums">{$t('git.filePosition', { index: index + 1, total: initialFiles.length })}</span>
    <Button variant="ghost" size="icon-sm" class="rounded-full" disabled={index >= initialFiles.length - 1} onclick={() => { index += 1 }} aria-label={$t('git.nextFile')}><CaretRightIcon class="size-4" /></Button>
    <span class="mx-1 h-5 w-px bg-border"></span>
    <Button variant={wrap ? 'secondary' : 'ghost'} size="icon-sm" class="rounded-full" onclick={() => { wrap = !wrap }} aria-label={$t('git.wrapLines')} title={$t('git.wrapLines')}><TextAlignLeftIcon class="size-4" /></Button>
  </div>
</div>
