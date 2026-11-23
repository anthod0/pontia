<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { FileActionLook } from './fileActionButton'
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import FileXIcon from 'phosphor-svelte/lib/FileXIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import FileAction from './FileAction.svelte'
  import { observeFile, type SharedFileState } from './sharedFileState.svelte'

  /** One image of an all-image share, cropped to fill its cell, with its download state drawn on it. */
  let { itemActions, state, more = 0, highlighted = false, onenlarge, onmore, itemProps = {}, class: className = '' }: {
    itemActions?: Snippet<[FileActionLook]>
    state: SharedFileState
    /** Images hidden after this one; the tile then expands the card instead of opening the viewer. */
    more?: number
    highlighted?: boolean
    onenlarge: () => void
    onmore: () => void
    itemProps?: HTMLAttributes<HTMLDivElement>
    class?: string
  } = $props()
  const name = $derived(state.file.name)
  const failed = $derived(state.task?.status === 'failed' && !state.missing)
  // The preview request failed, or the content is not a previewable image.
  const previewFailed = $derived(state.imageUrl === null || (!state.task && state.failure !== null))
</script>

<div use:observeFile={state} {...itemProps} class={['group/tile relative overflow-hidden bg-background', className, itemProps.class]} title={name} data-shared-file={state.file.path}>
  {#if more}
    <!-- The last collapsed tile always expands the card, whatever its own image's state. -->
    <button type="button" class="absolute inset-0 block outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset" onclick={onmore} aria-label={$t('files.showMoreImages', { count: more })}>
      {#if state.imageUrl}<img src={state.imageUrl} alt={name} class="size-full object-cover" />{/if}
      <span class="absolute inset-0 grid place-items-center bg-black/50 text-2xl font-semibold text-white">+{more}</span>
    </button>
  {:else if state.missing}
    <div class="absolute inset-0 grid place-items-center text-muted-foreground" role="img" aria-label={`${name}: ${$t('files.missing')}`}>
      <div class="flex flex-col items-center gap-1 px-2 text-center"><FileXIcon class="size-6" /><span class="text-[11px] leading-4">{$t('files.missing')}</span></div>
    </div>
  {:else}
    {#if state.imageUrl}
      <button type="button" class="absolute inset-0 block outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset" onclick={onenlarge} aria-label={$t('files.enlargeNamed', { name })}>
        <img src={state.imageUrl} alt={name} class={['size-full object-cover', failed && 'brightness-50']} />
      </button>
    {:else if previewFailed}
      <!-- The preview could not load, or the file is no longer an image. -->
      <div class="absolute inset-0 flex flex-col items-center justify-center gap-1 px-2 text-center text-muted-foreground">
        <WarningCircleIcon class="size-6" />
        {#if !failed && !state.receiving}<span class="line-clamp-2 text-[11px] leading-4 [overflow-wrap:anywhere]">{state.failure ?? $t('files.previewNotImage')}</span>{/if}
      </div>
    {:else}
      <Skeleton class="absolute inset-0 rounded-none" role="status" aria-label={$t('files.loadingPreview')} />
    {/if}
    {#if state.receiving}
      <div class="pointer-events-none absolute inset-0 grid place-items-center bg-black/25">
        <div class="pointer-events-auto"><FileAction {state} look="overlay" /></div>
      </div>
      {#if state.percent !== null}<span class="pointer-events-none absolute bottom-1.5 left-1.5 rounded-md bg-black/55 px-1.5 py-0.5 text-[11px] font-medium text-white tabular-nums">{Math.floor(state.percent)}%</span>{/if}
    {:else if failed}
      <div class={['pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-1.5 px-2 text-center', state.imageUrl ? 'text-white' : 'text-destructive']}>
        <button type="button" class="pointer-events-auto grid size-9 place-items-center rounded-full bg-black/55 text-white outline-none backdrop-blur-sm hover:bg-black/70 focus-visible:ring-2 focus-visible:ring-ring" onclick={() => state.download()} aria-label={$t('downloads.retryNamed', { name })} title={state.failure ?? undefined}><ArrowClockwiseIcon class="size-4" /></button>
        <span class="text-[11px] font-medium">{$t('downloads.failed')}</span>
      </div>
    {:else}
      <div class={['absolute right-1.5 bottom-1.5 transition-opacity', state.imageUrl && 'opacity-0 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100 pointer-coarse:hidden']}><FileAction {state} look="overlay" /></div>
    {/if}
  {/if}
  <!-- With a mouse the tile's actions show while it is hovered or keyboard-focused, and while a menu is open; touch always shows them. -->
  {#if itemActions}<div class="absolute top-1.5 right-1.5 opacity-0 transition-opacity group-hover/tile:opacity-100 group-has-focus-visible/tile:opacity-100 has-aria-expanded:opacity-100 pointer-coarse:opacity-100">{@render itemActions('overlay')}</div>{/if}
  {#if highlighted}<div class="pointer-events-none absolute inset-0 ring-3 ring-primary ring-inset"></div>{/if}
</div>
