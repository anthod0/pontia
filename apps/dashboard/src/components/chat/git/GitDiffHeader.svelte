<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { splitChangePath } from './gitChanges'

  /** The bars above a file's diff: back, file name and refresh, then what kind of change it is. */
  let { path, origPath, backLabel, fetching, additions, deletions, badge, onback, onrefresh }: {
    path: string
    origPath: string | undefined
    backLabel: string
    fetching: boolean
    additions: number | null | undefined
    deletions: number | null | undefined
    badge: Snippet
    onback: () => void
    onrefresh: () => void
  } = $props()

  const parts = $derived(splitChangePath(path))
</script>

<header class="flex h-12 shrink-0 items-center gap-1 border-b px-2 max-lg:h-14">
  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" onclick={onback} aria-label={backLabel} title={backLabel}><ArrowLeftIcon class="size-4 max-lg:size-5" /></Button>
  <div class="flex min-w-0 flex-1 items-baseline gap-2 px-1" title={origPath ? `${origPath} → ${path}` : path}>
    <h2 class="min-w-0 shrink truncate text-sm font-medium text-heading">{parts.name}</h2>
    {#if parts.dir}<span class="min-w-0 flex-1 truncate text-left font-mono text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>{/if}
  </div>
  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" disabled={fetching} onclick={onrefresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
    <ArrowClockwiseIcon class={['size-4 max-lg:size-5', fetching && 'animate-spin']} />
  </Button>
</header>
<div class="flex shrink-0 items-center gap-2 border-b px-3 py-1.5 text-xs">
  {@render badge()}
  {#if origPath}<span class="min-w-0 truncate font-mono text-muted-foreground" title={origPath}>{$t('git.renamedFrom', { path: origPath })}</span>{/if}
  {#if additions || deletions}
    <span class="ml-auto flex shrink-0 gap-1.5 font-mono tabular-nums">
      {#if additions}<span class="text-success">+{additions}</span>{/if}
      {#if deletions}<span class="text-destructive">−{deletions}</span>{/if}
    </span>
  {/if}
</div>
