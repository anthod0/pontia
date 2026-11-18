<script lang="ts">
  import { t } from '$lib/i18n';
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import type { SessionFilesStore } from '../../stores/sessionFiles'
  import SharedFileCard from './SharedFileCard.svelte'
  import SharedImageViewer from './SharedImageViewer.svelte'
  import { canPreviewImage } from '$lib/downloads/preview'

  let { sessionId, files: store, open = $bindable(false) }: { sessionId: string; files: SessionFilesStore; open?: boolean } = $props()
  const files = $derived($store.files)
  const loading = $derived($store.loading)
  const error = $derived($store.error)
  const images = $derived(files.filter(canPreviewImage))
  let imageViewerOpen = $state(false)
  let imageViewerIndex = $state(0)
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[80svh] max-w-lg flex-col">
    <Dialog.Header>
      <Dialog.Title>{$t('files.shared')}</Dialog.Title>
      <Dialog.Description>{$t('files.sharedDescription')}</Dialog.Description>
    </Dialog.Header>
    {#if error}
      <div class="flex items-center gap-2">
        <p role="alert" class="text-sm text-destructive">{error}</p>
        <Button size="sm" variant="outline" onclick={() => store.retry()} disabled={loading}>{$t('common.retry')}</Button>
      </div>
    {/if}
    {#if files.length > 0}
      <ul class="-mx-2 min-h-0 overflow-y-auto">
        {#each files as file (file.file_id)}<li><SharedFileCard {file} {sessionId} variant="row" onenlarge={() => { imageViewerIndex = images.indexOf(file); imageViewerOpen = true }} /></li>{/each}
      </ul>
    {:else if loading}
      <p role="status" class="text-sm text-muted-foreground">{$t('files.loadingShared')}</p>
    {:else if !error}
      <p class="text-sm text-muted-foreground">{$t('files.noShared')}</p>
    {/if}
    <SharedImageViewer {sessionId} files={images} bind:open={imageViewerOpen} bind:index={imageViewerIndex} />
  </Dialog.Content>
</Dialog.Root>
