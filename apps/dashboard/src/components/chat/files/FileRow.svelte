<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { FileActionLook } from './fileActionButton'
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../device/context'
  import { useFilePreview } from '../filePreviewContext'
  import { fileKind } from '../fileKind'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import FileXIcon from 'phosphor-svelte/lib/FileXIcon'
  import FileDetails from './FileDetails.svelte'
  import FileActions from './FileActions.svelte'
  import { observeFile, type SharedFileState } from './sharedFileState.svelte'

  /** One file as an icon or thumbnail, its name and status, and its download action. */
  let { itemActions, state, onenlarge, class: className = '', ...rest }: HTMLAttributes<HTMLDivElement> & {
    itemActions?: Snippet<[FileActionLook]>
    state: SharedFileState
    /** Opens the image viewer at this file. */
    onenlarge?: () => void
  } = $props()
  const device = useDeviceRuntime()
  const preview = useFilePreview()
  const kind = $derived(fileKind(state.extension))
  const thumbnail = $derived(state.showImage && onenlarge)
</script>

<div use:observeFile={state} class={['flex items-center gap-2', state.missing && 'opacity-80', className]} data-file-row {...rest}>
  {#snippet icon()}
    {#if state.missing}
      <div class="grid size-11 shrink-0 place-items-center rounded-item bg-background text-muted-foreground"><FileXIcon class="size-6" /></div>
    {:else}
      <div class={['grid size-11 shrink-0 place-items-center rounded-item', kind.tone]}><kind.icon class="size-6" weight="duotone" /></div>
    {/if}
  {/snippet}
  {#if thumbnail}
    <div class="flex min-w-0 flex-1 items-center gap-3">
      {#if state.imageUrl}
        <button type="button" class="shrink-0 rounded-item outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={onenlarge} aria-label={$t('files.enlargeNamed', { name: state.file.name })}>
          <img src={state.imageUrl} alt={state.file.name} class="size-11 rounded-item object-cover" />
        </button>
      {:else}
        <Skeleton class="size-11 shrink-0 rounded-item" role="status" aria-label={$t('files.loadingPreview')} />
      {/if}
      <div class="min-w-0 flex-1"><FileDetails {state} /></div>
    </div>
  {:else if state.missing || (!preview?.allSharedFiles && (state.image || !kind.text))}
    <div class="flex min-w-0 flex-1 items-center gap-3">
      {@render icon()}
      <div class="min-w-0 flex-1"><FileDetails {state} /></div>
    </div>
  {:else}
    <button type="button" class="-m-1 flex min-w-0 flex-1 items-center gap-3 rounded-item p-1 text-left outline-none hover:bg-background/60 focus-visible:ring-2 focus-visible:ring-ring" onclick={() => preview.open({ device, sessionId: state.sessionId, path: state.file.path })} aria-label={$t('files.previewNamed', { name: state.file.name })}>
      {@render icon()}
      <div class="min-w-0 flex-1"><FileDetails {state} /></div>
    </button>
  {/if}
  <FileActions {itemActions} {state} />
</div>
