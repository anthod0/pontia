<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { FileActionLook } from './fileActionButton'
  import type { HTMLAttributes } from 'svelte/elements'
  import { tick, untrack } from 'svelte'
  import { t, number } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../device/context'
  import SharedImageViewer from '../SharedImageViewer.svelte'
  import FilesIcon from 'phosphor-svelte/lib/FilesIcon'
  import ListIcon from 'phosphor-svelte/lib/ListIcon'
  import SquaresFourIcon from 'phosphor-svelte/lib/SquaresFourIcon'
  import { useFilePreview } from '../filePreviewContext'
  import FileBubble from './FileBubble.svelte'
  import FileList from './FileList.svelte'
  import ImageMosaic from './ImageMosaic.svelte'
  import DownloadAllButton from './DownloadAllButton.svelte'
  import { SharedFileState, formatFileSize } from './sharedFileState.svelte'
  import { hiddenUntilExpanded } from './layout'
  import { attachedFile } from '$lib/attachmentBlocks'
  import type { AttachedFile } from '../../../api/types'

  /**
   * The files of one attachments block as a single card: one file keeps the single-file bubble, files
   * that can all be previewed as images become a mosaic, and anything else a list.
   */
  let { itemActions, paths, sessionId, highlighted = null, itemProps = () => ({}) }: {
    itemActions?: Snippet<[FileActionLook]>
    /** Absolute paths, in order and without duplicates. */
    paths: string[]
    sessionId: string
    /** Path to mark, such as a quote's jump target; a hidden file is revealed first. */
    highlighted?: string | null
    /** Attributes for the element that stands for each file, such as quote targets. */
    itemProps?: (file: AttachedFile) => HTMLAttributes<HTMLDivElement>
  } = $props()
  const preview = useFilePreview()
  const device = useDeviceRuntime()
  // Refreshed file lists keep each file's state, such as its loaded preview.
  const cache = new Map<string, SharedFileState>()
  const states = $derived(paths.map(path => {
    let state = cache.get(path)
    if (!state || state.sessionId !== sessionId) cache.set(path, state = new SharedFileState(device, sessionId, attachedFile(path)))
    return state
  }))
  const files = $derived(states.map(state => state.file))
  const allImages = $derived(states.length > 1 && states.every(state => state.image))
  let showList = $state(false)
  let expanded = $state(false)
  let root = $state<HTMLElement>()
  // The total shows once every file's size is known.
  const total = $derived(states.every(state => state.info) ? states.reduce((sum, state) => sum + state.info!.size, 0) : null)
  const receiving = $derived(states.filter(state => state.receiving).length)
  const failed = $derived(states.filter(state => state.task?.status === 'failed' && !state.missing).length)
  const missing = $derived(states.filter(state => state.missing).length)
  const segments = $derived([
    ...(total !== null ? [{ text: formatFileSize(total, $number), alert: false }] : []),
    ...(receiving ? [{ text: $t('files.receivingCount', { count: receiving }), alert: false }] : []),
    ...(failed ? [{ text: $t('files.failedCount', { count: failed }), alert: true }] : []),
    ...(missing ? [{ text: $t('files.missingCount', { count: missing }), alert: false }] : []),
  ])
  const stateItemProps = (state: SharedFileState) => itemProps(state.file)

  // Viewer images are fixed when it opens so a file found missing meanwhile does not shift the index.
  let viewerFiles = $state<AttachedFile[]>([])
  let viewerIndex = $state(0)
  let viewerOpen = $state(false)
  function enlarge(state: SharedFileState) {
    preview?.rememberImage?.({ sessionId, path: state.file.path })
    const images = states.filter(other => other.image && (!other.missing || other === state))
    viewerFiles = images.map(other => other.file)
    viewerIndex = images.indexOf(state)
    viewerOpen = true
  }

  $effect(() => {
    const id = highlighted
    if (!id) return
    const index = paths.indexOf(id)
    if (index === -1) return
    // Only a new highlight expands the card; switching views while it shows does not.
    if (untrack(() => hiddenUntilExpanded(index, files.length, allImages && !showList))) expanded = true
    void tick().then(() => root?.querySelector(`[data-shared-file="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' }))
  })
</script>

{#if states.length === 1}
  <FileBubble {itemActions} state={states[0]} highlighted={highlighted === states[0].file.path} onenlarge={() => enlarge(states[0])} itemProps={stateItemProps(states[0])} />
{:else if allImages}
  <figure bind:this={root} class="w-80 max-w-full overflow-hidden rounded-2xl bg-muted" data-shared-files>
    {#if showList}
      <div class="p-1"><FileList {itemActions} {states} bind:expanded {highlighted} onenlarge={enlarge} itemProps={stateItemProps} /></div>
    {:else}
      <ImageMosaic {itemActions} {states} bind:expanded {highlighted} onenlarge={enlarge} itemProps={stateItemProps} />
    {/if}
    <figcaption class="flex items-center gap-2 px-3 py-2">
      <div class="min-w-0 flex-1">
        <p class="truncate text-sm font-medium text-heading">{$t('files.imageCount', { count: files.length })}</p>
        <p class="text-xs leading-4 text-muted-foreground tabular-nums">
          {#each segments as segment, i (segment.text)}{#if i}{' · '}{/if}<span class={['whitespace-nowrap', segment.alert && 'text-destructive']}>{segment.text}</span>{/each}
        </p>
      </div>
      <button type="button" class="inline-flex size-8 shrink-0 items-center justify-center rounded-full p-0 text-muted-foreground outline-none hover:bg-background hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" aria-label={showList ? $t('files.showAsImages') : $t('files.showAsList')} title={showList ? $t('files.showAsImages') : $t('files.showAsList')} aria-pressed={showList} onclick={() => { showList = !showList; expanded = false }}>
        {#if showList}<SquaresFourIcon class="size-4" />{:else}<ListIcon class="size-4" />{/if}
      </button>
      <DownloadAllButton {states} />
    </figcaption>
  </figure>
{:else}
  <div bind:this={root} class="w-80 max-w-full rounded-2xl bg-muted p-1" data-shared-files>
    <div class="flex items-center gap-2 px-1.5 pt-0.5 pb-1">
      <FilesIcon class="size-4 shrink-0 text-muted-foreground" />
      <p class="min-w-0 flex-1 text-xs leading-4 text-muted-foreground tabular-nums">
        {[$t('files.fileCount', { count: files.length }), total !== null && formatFileSize(total, $number), receiving && $t('files.receivingCount', { count: receiving })].filter(Boolean).join(' · ')}
      </p>
      <DownloadAllButton {states} />
    </div>
    <FileList {itemActions} {states} bind:expanded {highlighted} onenlarge={enlarge} itemProps={stateItemProps} />
  </div>
{/if}
{#if viewerFiles.length}
  <SharedImageViewer {sessionId} files={viewerFiles} bind:open={viewerOpen} bind:index={viewerIndex} />
{/if}
