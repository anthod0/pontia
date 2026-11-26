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
  import { wordHighlights, type GitDiffContent } from './gitChanges'

  /** The body of one file's diff: hunks with line numbers, or why there are none. */
  let { path, diff, error, onretry, compact = false, wrap = false }: {
    compact?: boolean
    wrap?: boolean
    path: string
    diff: GitDiffContent | undefined
    /** Set when the diff could not be read. */
    error: Error | null
    onretry: () => void
  } = $props()

  const skeletonLines = [42, 68, 55, 0, 74, 61, 83, 36, 0, 58, 70, 47]
  const highlighted = $derived(diff ? highlightDiff(diff) : null)
  const words = $derived(diff ? wordHighlights(diff.hunks) : new Map())

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

  const lineClass = { context: '', add: 'diff-add', delete: 'diff-delete' }
  const markClass = { context: '', add: 'text-git-added', delete: 'text-git-deleted' }
  const marks = { context: ' ', add: '+', delete: '−' }
</script>

<div class="diff-shell min-h-0 flex-1 overflow-auto overscroll-contain" data-git-diff class:compact class:wrap>
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
    <div class={['shiki font-mono text-xs leading-5', !wrap && 'min-w-max']}>
      {#each diff.hunks as hunk, hunkIndex (hunkIndex)}
        <div class="diff-hunk flex text-muted-foreground">
          <span class="sticky left-0 line-numbers shrink-0"></span>
          <span class="px-3 py-0.5 whitespace-pre">{hunk.header}</span>
        </div>
        {#each hunk.lines as line, lineIndex (lineIndex)}
          {@const word = words.get(line)}
          <div class={['flex', lineClass[line.kind]]}>
            <span class={['gutter sticky left-0 flex line-numbers shrink-0 text-right text-muted-foreground/70 tabular-nums select-none', lineClass[line.kind], markClass[line.kind]]}>
              <span class="old-line">{line.old ?? ''}</span>
              <span class="new-line">{line.new ?? ''}</span>
              <span class="single-line">{line.new ?? line.old ?? ''}</span>
            </span>
            <span class={['w-5 shrink-0 text-center select-none', markClass[line.kind]]} aria-hidden="true">{marks[line.kind]}</span>
            <span class="code-cell pr-4 whitespace-pre" data-code>
              {#if word}
                {line.text.slice(0, word.start)}<mark class={line.kind === 'add' ? 'word-add' : 'word-delete'}>{line.text.slice(word.start, word.end)}</mark>{line.text.slice(word.end)}
              {:else if highlighted?.get(line) !== undefined}{@html highlighted.get(line)}{:else}{line.text}{/if}
            </span>
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
.diff-shell { container-type: inline-size; }
.line-numbers { width: 8ch; font-size: 11px; }
.line-numbers > span { width: 4ch; padding-right: .5ch; }
.single-line { display: none; }
.diff-add { background: color-mix(in srgb, var(--diff-add) 13%, transparent); }
.diff-delete { background: color-mix(in srgb, var(--diff-del) 9%, transparent); }
.gutter { background: var(--surface); }
.gutter.diff-add { border-left: 2px solid var(--git-added); background: color-mix(in srgb, var(--diff-add) 22%, var(--surface)); }
.gutter.diff-delete { border-left: 2px solid var(--git-deleted); background: color-mix(in srgb, var(--diff-del) 16%, var(--surface)); }
.diff-hunk, .diff-hunk .line-numbers { background: color-mix(in srgb, var(--muted) 70%, var(--surface)); }
.word-add { color: inherit; background: color-mix(in srgb, var(--diff-add) 30%, transparent); }
.word-delete { color: inherit; background: color-mix(in srgb, var(--diff-del) 24%, transparent); }
.wrap .code-cell { white-space: pre-wrap; overflow-wrap: anywhere; padding-left: 2ch; text-indent: -2ch; }
@container (width < 600px) {
  .line-numbers { width: 4ch; }
  .old-line, .new-line { display: none; }
  .single-line { display: block; width: 4ch; }
}
.compact .line-numbers { width: 4ch; }
.compact .old-line, .compact .new-line { display: none; }
.compact .single-line { display: block; width: 4ch; }
:global(.dark) .diff-add { background: color-mix(in srgb, var(--diff-add) 11%, transparent); }
:global(.dark) .diff-delete { background: color-mix(in srgb, var(--diff-del) 9%, transparent); }
:global(.dark) .gutter.diff-add { background: color-mix(in srgb, var(--diff-add) 18%, var(--surface)); }
:global(.dark) .gutter.diff-delete { background: color-mix(in srgb, var(--diff-del) 18%, var(--surface)); }
</style>
