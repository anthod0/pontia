<script lang="ts">
  import { onDestroy } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { filePreview, PREVIEW_LIMIT, PREVIEW_TYPES } from '$lib/downloads/preview'
  import { saveDownload, type DownloadProgress } from '$lib/downloads/save'
  import { sharedFileContent } from '../../api/sessionFiles'
  import { ApiError } from '../../api/errors'
  import type { SharedFile } from '../../api/types'

  let { file, sessionId }: { file: SharedFile; sessionId: string } = $props()
  let previewUrl = $state<string | null>(null)
  let enlarged = $state(false)
  let downloading = $state(false)
  let progress = $state<DownloadProgress | null>(null)
  let error = $state<string | null>(null)
  let destroyed = false

  onDestroy(() => {
    destroyed = true
    if (previewUrl) URL.revokeObjectURL(previewUrl)
  })

  function showError(cause: unknown) {
    error = cause instanceof ApiError && cause.status === 404
      ? 'File no longer exists'
      : cause instanceof Error ? cause.message : String(cause)
  }

  function observePreview(node: HTMLElement) {
    if (file.size > PREVIEW_LIMIT || !PREVIEW_TYPES.has(file.media_type)) return
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return
      observer.disconnect()
      void filePreview(sessionId, file.file_id).then(blob => {
        if (blob && !destroyed) previewUrl = URL.createObjectURL(blob)
      }).catch(showError)
    })
    observer.observe(node)
    return { destroy: () => observer.disconnect() }
  }

  async function download() {
    downloading = true
    error = null
    progress = { received: 0, total: null }
    try {
      await saveDownload(await sharedFileContent(sessionId, file.file_id), file.name, value => { progress = value })
    } catch (cause) {
      showError(cause)
    } finally {
      downloading = false
      progress = null
    }
  }

  function sizeLabel(size: number) {
    return size < 1024 ? `${size} B` : size < 1024 * 1024
      ? `${(size / 1024).toFixed(1)} KiB` : `${(size / 1024 / 1024).toFixed(1)} MiB`
  }
</script>

<div use:observePreview>
  <Card.Root size="sm" class="gap-2 py-2">
    <Card.Content class="flex items-center gap-2 px-2">
      {#if previewUrl}
        <button type="button" class="shrink-0" onclick={() => { enlarged = true }} aria-label={`Enlarge ${file.name}`}>
          <img src={previewUrl} alt={file.name} class="size-10 rounded object-cover" />
        </button>
      {:else}
        <div class="flex size-10 shrink-0 items-center justify-center rounded bg-muted"><FileIcon class="size-5 text-muted-foreground" aria-hidden="true" /></div>
      {/if}
      <div class="min-w-0 flex-1">
        <Card.Title class="truncate text-sm" title={file.name}>{file.name}</Card.Title>
        <Card.Description class="truncate text-xs" title={new Date(file.created_at).toLocaleString()}>{sizeLabel(file.size)}</Card.Description>
      </div>
      <Button size="icon-sm" variant="ghost" onclick={download} disabled={downloading} aria-label={downloading ? `Receiving ${file.name}` : `Download ${file.name}`} title={downloading ? 'Receiving…' : 'Download'}>
        <DownloadSimpleIcon class="size-4" />
      </Button>
    </Card.Content>
    {#if progress || error}
      <div class="space-y-1 px-2">
        {#if progress}
          <div class="space-y-1" role="status">
            <Progress value={progress.total === null ? null : progress.total === 0 ? 100 : progress.received / progress.total * 100} aria-label="File reception progress" />
            <p class="text-xs text-muted-foreground">{sizeLabel(progress.received)}{progress.total !== null ? ` / ${sizeLabel(progress.total)}` : ''}</p>
          </div>
        {/if}
        {#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
      </div>
    {/if}
  </Card.Root>
</div>

<Dialog.Root bind:open={enlarged}>
  <Dialog.Content class="max-w-4xl">
    <Dialog.Header>
      <Dialog.Title class="break-all">{file.name}</Dialog.Title>
      <Dialog.Description>Shared image</Dialog.Description>
    </Dialog.Header>
    {#if previewUrl}<img src={previewUrl} alt={file.name} class="max-h-[75vh] w-full object-contain" />{/if}
  </Dialog.Content>
</Dialog.Root>
