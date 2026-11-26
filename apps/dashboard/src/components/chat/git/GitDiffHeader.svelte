<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CaretUpIcon from 'phosphor-svelte/lib/CaretUpIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { changeDirLabel, splitChangePath } from './gitChanges'

  let { path, origPath, backLabel, fetching, additions, deletions, index, total, status, statusTone, context, onback, onprevious, onnext, onrefresh }: {
    path: string
    origPath: string | undefined
    backLabel?: string
    fetching: boolean
    additions: number | null | undefined
    deletions: number | null | undefined
    index?: number
    total?: number
    status?: string
    statusTone?: string
    context: Snippet
    onback?: () => void
    onprevious?: () => void
    onnext?: () => void
    onrefresh: () => void
  } = $props()

  const parts = $derived(splitChangePath(path))
</script>

<header class="flex h-[52px] shrink-0 items-center gap-2 border-b px-2">
  {#if onback}<Button variant="ghost" size="icon-circle-sm" class="p-0 max-lg:size-11" onclick={onback} aria-label={backLabel} title={backLabel}><ArrowLeftIcon class="size-4 max-lg:size-5" /></Button>{/if}
  <div class="min-w-0 flex-1" title={origPath ? `${origPath} → ${path}` : path}>
    <h2 class="flex min-w-0 items-center gap-1.5 truncate text-13 font-medium text-heading">{#if status}<span class={['font-mono text-11', statusTone]}>{status}</span>{/if}<span class="truncate">{parts.name}</span></h2>
    <div class="flex min-w-0 items-center gap-1 text-11 text-muted-foreground">
      {@render context()}
      {#if index !== undefined && total}<span aria-hidden="true">·</span><span class="tabular-nums">{$t('git.filePosition', { index: index + 1, total })}</span>{/if}
      {#if origPath || parts.dir}
        <span aria-hidden="true">·</span>
        <span class="truncate font-mono">{changeDirLabel({ path, orig_path: origPath })}</span>
      {/if}
    </div>
  </div>
  {#if additions || deletions}
    <span class="ml-auto flex shrink-0 gap-1.5 font-mono text-xs tabular-nums">
      {#if additions}<span class="text-git-added">+{additions}</span>{/if}
      {#if deletions}<span class="text-git-deleted">−{deletions}</span>{/if}
    </span>
  {/if}
  {#if total && total > 1}
    <Button variant="ghost" size="icon-circle-sm" class="p-0" disabled={index === 0} onclick={onprevious} aria-label={$t('git.previousFile')} title={$t('git.previousFile')}><CaretUpIcon class="size-4" /></Button>
    <Button variant="ghost" size="icon-circle-sm" class="p-0" disabled={index === total - 1} onclick={onnext} aria-label={$t('git.nextFile')} title={$t('git.nextFile')}><CaretDownIcon class="size-4" /></Button>
  {/if}
  <Button variant="ghost" size="icon-circle-sm" class="p-0 max-lg:size-11" disabled={fetching} onclick={onrefresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
    <ArrowClockwiseIcon class={['size-4 max-lg:size-5', fetching && 'animate-spin']} />
  </Button>
</header>
