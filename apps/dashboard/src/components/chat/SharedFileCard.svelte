<script lang="ts">
  import { onDestroy } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
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
  <Card.Root class="gap-3 py-4">
    <Card.Header class="px-4">
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0">
          <Card.Title class="break-all text-sm">{file.name}</Card.Title>
          <Card.Description class="mt-1">{sizeLabel(file.size)} · <time datetime={file.created_at}>{new Date(file.created_at).toLocaleString()}</time></Card.Description>
        </div>
        <Button size="sm" variant="outline" onclick={download} disabled={downloading}>{downloading ? 'Receiving…' : 'Download'}</Button>
      </div>
    </Card.Header>
    <Card.Content class="space-y-3 px-4">
      {#if previewUrl}
        <button type="button" class="block max-w-full" onclick={() => { enlarged = true }} aria-label={`Enlarge ${file.name}`}>
          <img src={previewUrl} alt={file.name} class="max-h-48 rounded object-contain" />
        </button>
      {/if}
      {#if progress}
        <div class="space-y-1" role="status">
          <Progress value={progress.total === null ? null : progress.total === 0 ? 100 : progress.received / progress.total * 100} aria-label="File reception progress" />
          <p class="text-xs text-muted-foreground">{sizeLabel(progress.received)}{progress.total !== null ? ` / ${sizeLabel(progress.total)}` : ''}</p>
        </div>
      {/if}
      {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
      <Collapsible.Root>
        <Collapsible.Trigger class="text-xs text-muted-foreground underline underline-offset-4">Show full path</Collapsible.Trigger>
        <Collapsible.Content><p class="mt-2 break-all font-mono text-xs">{file.path}</p></Collapsible.Content>
      </Collapsible.Root>
    </Card.Content>
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
