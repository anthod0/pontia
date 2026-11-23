<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import BinaryIcon from 'phosphor-svelte/lib/BinaryIcon'
  import FileDashedIcon from 'phosphor-svelte/lib/FileDashedIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { highlightCodeLines } from '$lib/components/ai-elements/message/markdownHighlighter'
  import { textFileLanguage } from '$lib/downloads/textPreview'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile, GitDiffLine, GitDiffView } from '../../../api/types'
  import type { GitDiffTarget } from '../../../queries/workspaces'
  import { gitChangeGroups, splitChangePath } from './gitChanges'

  let { device, workspaceId, file, change, onback, onclose }: {
    device: DeviceRuntime
    workspaceId: string
    file: GitDiffTarget
    /** The file's entry in the change list, when it is still listed. */
    change: GitChangedFile | undefined
    onback: () => void
    /** Closes the surrounding panel; the close button shows only with it. */
    onclose?: () => void
  } = $props()

  const skeletonLines = [42, 68, 55, 0, 74, 61, 83, 36, 0, 58, 70, 47]
  // The panel recreates this view for each file and device.
  // svelte-ignore state_referenced_locally
  const diffQuery = device.workspaceQueries.createWorkspaceGitDiffQuery(() => workspaceId, () => file)
  const parts = $derived(splitChangePath(file.path))
  const group = $derived(gitChangeGroups.find(item => item.group === file.group)!)
  // The list's counts cover the whole file; a truncated diff holds only part of it.
  const additions = $derived(change ? change.additions : diffQuery.data && countLines(diffQuery.data, 'add'))
  const deletions = $derived(change ? change.deletions : diffQuery.data && countLines(diffQuery.data, 'delete'))
  const highlighted = $derived(diffQuery.data ? highlightDiff(diffQuery.data) : null)

  function countLines(diff: GitDiffView, kind: GitDiffLine['kind']): number {
    return diff.hunks.reduce((total, hunk) => total + hunk.lines.filter(line => line.kind === kind).length, 0)
  }

  /** Highlights the old and new sides separately so each line keeps its own syntax state. */
  function highlightDiff(diff: GitDiffView): Map<GitDiffLine, string> | null {
    const language = textFileLanguage(file.path)
    const sides = (['old', 'new'] as const).map(side => diff.hunks.flatMap(hunk => hunk.lines.filter(line => line[side] !== null)))
    const html = sides.map(lines => highlightCodeLines(lines.map(line => line.text).join('\n'), language))
    if (!html[0] || !html[1]) return null
    const result = new Map<GitDiffLine, string>()
    sides.forEach((lines, index) => lines.forEach((line, row) => {
      if (!result.has(line)) result.set(line, html[index]![row] ?? '')
    }))
    return result
  }

  const lineClass = {
    context: '',
    add: 'bg-success/12',
    delete: 'bg-destructive/12',
  }
  const markClass = { context: '', add: 'text-success', delete: 'text-destructive' }
  const marks = { context: ' ', add: '+', delete: '−' }
</script>

<header class="flex h-12 shrink-0 items-center gap-1 border-b px-2 max-lg:h-14">
  <Button variant="ghost" size="icon-sm" class="max-lg:size-11" onclick={onback} aria-label={$t('git.backToChanges')} title={$t('git.backToChanges')}><ArrowLeftIcon class="size-4 max-lg:size-5" /></Button>
  <div class="flex min-w-0 flex-1 items-baseline gap-2 px-1" title={file.orig_path ? `${file.orig_path} → ${file.path}` : file.path}>
    <h2 class="min-w-0 shrink truncate text-sm font-medium text-heading">{parts.name}</h2>
    {#if parts.dir}<span class="min-w-0 flex-1 truncate text-left font-mono text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>{/if}
  </div>
  <Button variant="ghost" size="icon-sm" class="max-lg:size-11" disabled={diffQuery.isFetching} onclick={() => diffQuery.refetch()} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
    <ArrowClockwiseIcon class={['size-4 max-lg:size-5', diffQuery.isFetching && 'animate-spin']} />
  </Button>
  {#if onclose}<Button variant="ghost" size="icon-sm" class="max-lg:size-11" onclick={onclose} aria-label={$t('common.close')} title={`${$t('common.close')} (Alt G)`}><XIcon class="size-4 max-lg:size-5" /></Button>{/if}
</header>
<div class="flex shrink-0 items-center gap-2 border-b px-3 py-1.5 text-xs">
  <Badge variant="outline" class={['font-normal', group.tone]}>{$t(group.label)}</Badge>
  {#if file.orig_path}<span class="min-w-0 truncate font-mono text-muted-foreground" title={file.orig_path}>{$t('git.renamedFrom', { path: file.orig_path })}</span>{/if}
  {#if !diffQuery.data?.binary && (additions || deletions)}
    <span class="ml-auto flex shrink-0 gap-1.5 font-mono tabular-nums">
      {#if additions}<span class="text-success">+{additions}</span>{/if}
      {#if deletions}<span class="text-destructive">−{deletions}</span>{/if}
    </span>
  {/if}
</div>

<div class="min-h-0 flex-1 overflow-auto overscroll-contain" data-git-diff>
  {#if diffQuery.isError}
    <Empty.Root role="alert" class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-destructive/10 text-destructive"><WarningCircleIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.diffFailed')}</Empty.Title>
        <Empty.Description class="break-words font-mono text-xs">{diffQuery.error.message}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button variant="outline" size="sm" onclick={() => diffQuery.refetch()}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else if !diffQuery.data}
    <div role="status" aria-label={$t('git.loadingDiff')} class="space-y-2.5 p-4">
      {#each skeletonLines as width, index (index)}
        <Skeleton class="h-3 rounded-sm" style="width: {width}%; {width ? '' : 'visibility: hidden'}" />
      {/each}
    </div>
  {:else if diffQuery.data.binary}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><BinaryIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.binaryFile')}</Empty.Title>
      </Empty.Header>
    </Empty.Root>
  {:else if !diffQuery.data.hunks.length}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileDashedIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.noTextChanges')}</Empty.Title>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <div class="shiki min-w-max font-mono text-xs leading-5">
      {#each diffQuery.data.hunks as hunk, hunkIndex (hunkIndex)}
        <div class="flex bg-muted/60 text-muted-foreground">
          <span class="sticky left-0 w-[4.5rem] shrink-0 bg-muted"></span>
          <span class="px-3 py-0.5 whitespace-pre">{hunk.header}</span>
        </div>
        {#each hunk.lines as line, lineIndex (lineIndex)}
          <div class={['flex', lineClass[line.kind]]}>
            <span class="sticky left-0 flex w-[4.5rem] shrink-0 bg-surface text-right text-muted-foreground/70 tabular-nums select-none">
              <span class={['w-9 pr-1.5', lineClass[line.kind]]}>{line.old ?? ''}</span>
              <span class={['w-9 pr-1.5', lineClass[line.kind]]}>{line.new ?? ''}</span>
            </span>
            <span class={['w-5 shrink-0 text-center select-none', markClass[line.kind]]} aria-hidden="true">{marks[line.kind]}</span>
            <span class="pr-4 whitespace-pre">{#if highlighted?.get(line) !== undefined}{@html highlighted.get(line)}{:else}{line.text}{/if}</span>
          </div>
        {/each}
      {/each}
    </div>
    {#if diffQuery.data.truncated}
      <p class="sticky left-0 p-3 text-center text-xs text-muted-foreground">{$t('git.diffTruncated')}</p>
    {/if}
  {/if}
</div>
