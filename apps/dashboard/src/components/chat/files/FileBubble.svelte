<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import FileDetails from './FileDetails.svelte'
  import FileAction from './FileAction.svelte'
  import FileRow from './FileRow.svelte'
  import { observePreview, type SharedFileState } from './sharedFileState.svelte'

  /** A share of one file: an image with a caption, or a file row. */
  let { itemActions, state, highlighted = false, onenlarge, itemProps = {} }: {
    itemActions?: Snippet
    state: SharedFileState
    highlighted?: boolean
    onenlarge: () => void
    /** Attributes for the element that stands for the file, such as quote targets. */
    itemProps?: HTMLAttributes<HTMLDivElement>
  } = $props()
</script>

<div use:observePreview={state} {...itemProps} class={['w-80 max-w-full rounded-2xl transition-shadow', highlighted && 'ring-2 ring-primary/50', itemProps.class]} data-shared-file={state.file.file_id}>
  {#if state.showImage}
    <figure class="overflow-hidden rounded-2xl bg-muted">
      {#if state.imageUrl}
        <button type="button" class="block w-full outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset" onclick={onenlarge} aria-label={$t('files.enlargeNamed', { name: state.file.name })}>
          <img src={state.imageUrl} alt={state.file.name} class="max-h-72 w-full object-cover" />
        </button>
      {:else}
        <Skeleton class="aspect-[16/10] w-full rounded-none" role="status" aria-label={$t('files.loadingPreview')} />
      {/if}
      <figcaption class="flex items-center gap-3 px-3 py-2">
        <div class="min-w-0 flex-1"><FileDetails {state} /></div>
        <FileAction {state} />
        {@render itemActions?.()}
      </figcaption>
    </figure>
  {:else}
    <FileRow {itemActions} {state} class="rounded-2xl bg-muted p-2.5" />
  {/if}
</div>
