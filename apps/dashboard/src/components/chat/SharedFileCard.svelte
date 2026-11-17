<script lang="ts">
  import { t, currentLocale, number } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { onDestroy } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Lightbox } from 'picola/svelte'
  import 'picola/picola.css'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { PREVIEW_LIMIT, PREVIEW_TYPES } from "$lib/downloads/preview";
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { ApiError } from '../../api/errors'
  import type { SharedFile } from '../../api/types'
  const device = useDeviceRuntime();
  const { filePreview } = device.previews;

  let { file, sessionId, variant = 'card' }: { file: SharedFile; sessionId: string; variant?: 'card' | 'row' } = $props()
  let previewUrl = $state<string | null>(null)
  let enlarged = $state(false)
  const key = $derived(downloadKey(device.id, sessionId, { kind: 'shared', fileId: file.file_id }))
  const task = $derived($downloads.find(task => task.key === key))
  const downloading = $derived(task?.status === 'receiving')
  const progress = $derived(downloading ? task!.progress : null)
  let error = $state<string | null>(null)
  let destroyed = false

  onDestroy(() => {
    destroyed = true
    if (previewUrl) URL.revokeObjectURL(previewUrl)
  })

  function showError(cause: unknown) {
    error = cause instanceof ApiError && cause.status === 404
      ? $t('files.missing')
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

  function download() {
    error = null
    downloads.start(device.id, sessionId, { kind: 'shared', fileId: file.file_id }, file.name)
  }

  function sizeLabel(size: number) {
    return size < 1024 ? `${$number(size)} B` : size < 1024 * 1024
      ? `${$number(size / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} KiB` : `${$number(size / 1024 / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MiB`
  }
</script>

{#snippet body(thumb: string)}
  <div class="flex items-center gap-2">
    {#if previewUrl}
      <button type="button" class="shrink-0" onclick={() => { enlarged = true }} aria-label={$t('files.enlargeNamed', { name: file.name })}>
        <img src={previewUrl} alt={file.name} class="{thumb} rounded object-cover" />
      </button>
    {:else}
      <div class="flex {thumb} shrink-0 items-center justify-center rounded bg-muted"><FileIcon class={variant === 'row' ? 'size-4 text-muted-foreground' : 'size-5 text-muted-foreground'} aria-hidden="true" /></div>
    {/if}
    <div class="min-w-0 flex-1">
      <p class="truncate text-sm font-medium" title={file.name}>{file.name}</p>
      <p class="truncate text-xs text-muted-foreground" title={new Date(file.created_at).toLocaleString($currentLocale)}>{sizeLabel(file.size)}</p>
    </div>
    <Button size="icon-sm" variant="ghost" onclick={download} disabled={downloading} aria-label={downloading ? $t('files.receivingNamed', { name: file.name }) : $t('files.downloadNamed', { name: file.name })} title={downloading ? $t('files.receiving') : $t('files.download')}>
      <DownloadSimpleIcon class="size-4" />
    </Button>
  </div>
  {#if progress || error || task?.error}
    <div class="space-y-1">
      {#if progress}
        <div class="space-y-1" role="status">
          <Progress value={progress.total === null ? null : progress.total === 0 ? 100 : progress.received / progress.total * 100} aria-label={$t('files.receptionProgress')} />
          <p class="text-xs text-muted-foreground">{sizeLabel(progress.received)}{progress.total !== null ? ` / ${sizeLabel(progress.total)}` : ''}</p>
        </div>
      {/if}
      {#if task?.error || error}<p class="text-xs text-destructive" role="alert">{task?.error || error}</p>{/if}
    </div>
  {/if}
{/snippet}

<div use:observePreview>
  {#if variant === 'row'}
    <div class="space-y-1 rounded-md px-2 py-1.5 hover:bg-muted/50">{@render body('size-8')}</div>
  {:else}
    <Card.Root size="sm" class="gap-2 px-2 py-2">{@render body('size-10')}</Card.Root>
  {/if}
</div>

<Dialog.Root bind:open={enlarged}>
  <Dialog.Content class="inset-0 block max-w-none translate-none rounded-none bg-black/90 p-0 text-white ring-0 sm:max-w-none" showCloseButton={false}>
    <Dialog.Title class="sr-only">{file.name}</Dialog.Title>
    {#if previewUrl}
      <Lightbox
        inline
        slides={[{ src: previewUrl, alt: file.name }]}
        zoom={{ max: 8 }}
        counter={false}
        labels={{ dialog: $t('files.sharedImage'), zoom: $t('files.toggleZoom') }}
      >
        {#snippet toolbar()}
          <Dialog.Close class="pcl__button" aria-label={$t('common.close')}><XIcon class="size-5" /></Dialog.Close>
        {/snippet}
      </Lightbox>
    {/if}
  </Dialog.Content>
</Dialog.Root>
