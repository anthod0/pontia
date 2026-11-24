<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import BinaryIcon from 'phosphor-svelte/lib/BinaryIcon'
  import FileDashedIcon from 'phosphor-svelte/lib/FileDashedIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { highlightCodeLines } from '$lib/components/ai-elements/message/markdownHighlighter'
  import { textFileLanguage } from '$lib/downloads/textPreview'
  import type { GitDiffLine } from '../../../api/types'
  import type { GitDiffContent } from './gitChanges'

  /** The body of one file's diff: hunks with line numbers, or why there are none. */
  let { path, diff, error, onretry, compact = false }: {
    compact?: boolean
    path: string
    diff: GitDiffContent | undefined
    /** Set when the diff could not be read. */
    error: Error | null
    onretry: () => void
  } = $props()

  const skeletonLines = [42, 68, 55, 0, 74, 61, 83, 36, 0, 58, 70, 47]
  const highlighted = $derived(diff ? highlightDiff(diff) : null)

  /** Highlights the old and new sides separately so each line keeps its own syntax state. */
  function highlightDiff(diff: GitDiffContent): Map<GitDiffLine, string> | null {
    const language = textFileLanguage(path)
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

<div class="min-h-0 flex-1 overflow-auto overscroll-contain" data-git-diff class:compact>
  {#if error}
    <Empty.Root role="alert" class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-destructive/10 text-destructive"><WarningCircleIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.diffFailed')}</Empty.Title>
        <Empty.Description class="break-words font-mono text-xs">{error.message}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button variant="outline" size="sm" onclick={onretry}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else if !diff}
    <div role="status" aria-label={$t('git.loadingDiff')} class="space-y-2.5 p-4">
      {#each skeletonLines as width, index (index)}
        <Skeleton class="h-3 rounded-sm" style="width: {width}%; {width ? '' : 'visibility: hidden'}" />
      {/each}
    </div>
  {:else if diff.binary}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><BinaryIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.binaryFile')}</Empty.Title>
      </Empty.Header>
    </Empty.Root>
  {:else if !diff.hunks.length}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileDashedIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.noTextChanges')}</Empty.Title>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <div class="shiki min-w-max font-mono text-xs leading-5">
      {#each diff.hunks as hunk, hunkIndex (hunkIndex)}
        <div class="flex bg-muted/60 text-muted-foreground">
          <span class="sticky left-0 line-numbers w-[4.5rem] shrink-0 bg-muted"></span>
          <span class="px-3 py-0.5 whitespace-pre">{hunk.header}</span>
        </div>
        {#each hunk.lines as line, lineIndex (lineIndex)}
          <div class={['flex', lineClass[line.kind]]}>
            <span class="sticky left-0 flex line-numbers w-[4.5rem] shrink-0 bg-surface text-right text-muted-foreground/70 tabular-nums select-none">
              <span class={['w-9 pr-1.5', lineClass[line.kind]]}>{line.old ?? ''}</span>
              <span class={['w-9 pr-1.5', lineClass[line.kind]]}>{line.new ?? ''}</span>
            </span>
            <span class={['w-5 shrink-0 text-center select-none', markClass[line.kind]]} aria-hidden="true">{marks[line.kind]}</span>
            <span class="pr-4 whitespace-pre">{#if highlighted?.get(line) !== undefined}{@html highlighted.get(line)}{:else}{line.text}{/if}</span>
          </div>
        {/each}
      {/each}
    </div>
    {#if diff.truncated}
      <p class="sticky left-0 p-3 text-center text-xs text-muted-foreground">{$t('git.diffTruncated')}</p>
    {/if}
  {/if}
</div>

<style>
.compact .line-numbers { width: 6.8ch; font-size: 11px; }
.compact .line-numbers > span { width: 3.4ch; padding-right: .3ch; }
</style>
