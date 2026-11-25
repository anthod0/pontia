<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { changeDirLabel, splitChangePath } from './gitChanges'

  let { path, origPath, backLabel, fetching, additions, deletions, context, onback, onrefresh }: {
    path: string
    origPath: string | undefined
    backLabel: string
    fetching: boolean
    additions: number | null | undefined
    deletions: number | null | undefined
    context: Snippet
    onback: () => void
    onrefresh: () => void
  } = $props()

  const parts = $derived(splitChangePath(path))
</script>

<header class="flex h-[52px] shrink-0 items-center gap-2 border-b px-2">
  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" onclick={onback} aria-label={backLabel} title={backLabel}><ArrowLeftIcon class="size-4 max-lg:size-5" /></Button>
  <div class="min-w-0 flex-1" title={origPath ? `${origPath} → ${path}` : path}>
    <h2 class="truncate text-sm font-medium text-heading">{parts.name}</h2>
    <div class="flex min-w-0 items-center gap-1 text-[11.5px] text-muted-foreground">
      {@render context()}
      {#if origPath || parts.dir}
        <span aria-hidden="true">·</span>
        <span class="truncate font-mono">{changeDirLabel({ path, orig_path: origPath })}</span>
      {/if}
    </div>
  </div>
  {#if additions || deletions}
    <span class="ml-auto flex shrink-0 gap-1.5 font-mono text-xs tabular-nums">
      {#if additions}<span class="text-success">+{additions}</span>{/if}
      {#if deletions}<span class="text-destructive">−{deletions}</span>{/if}
    </span>
  {/if}
  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" disabled={fetching} onclick={onrefresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
    <ArrowClockwiseIcon class={['size-4 max-lg:size-5', fetching && 'animate-spin']} />
  </Button>
</header>
