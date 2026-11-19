<script lang="ts">
  import { t, currentLocale, number } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { onDestroy } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { canPreviewImage } from "$lib/downloads/preview";
  import { useFilePreview } from './filePreviewContext'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { ApiError } from '../../api/errors'
  import type { SharedFile } from '../../api/types'
  const device = useDeviceRuntime();
  const preview = useFilePreview();

  let { file, sessionId, variant = 'card', onenlarge }: {
    file: SharedFile
    sessionId: string
    variant?: 'card' | 'row'
    /** Opens the image viewer at this file. */
    onenlarge?: () => void
  } = $props()
  let previewUrl = $state<string | null>(null)
  const key = $derived(downloadKey(device.id, sessionId, { kind: 'shared', fileId: file.file_id }))
  const task = $derived($downloads.find(task => task.key === key))
  const downloading = $derived(task?.status === 'receiving')
  const progress = $derived(downloading ? task!.progress : null)
  let error = $state<string | null>(null)
  let destroyed = false

  onDestroy(() => {
    destroyed = true
  })

  function showError(cause: unknown) {
    error = cause instanceof ApiError && cause.status === 404
      ? $t('files.missing')
      : cause instanceof Error ? cause.message : String(cause)
  }

  function observePreview(node: HTMLElement) {
    if (!canPreviewImage(file)) return
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return
      observer.disconnect()
      void device.previews.sharedImageUrl(sessionId, file.file_id).then(url => {
        if (!destroyed) previewUrl = url
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
  {#snippet icon()}
    <div class="flex {thumb} shrink-0 items-center justify-center rounded bg-muted"><FileIcon class={variant === 'row' ? 'size-4 text-muted-foreground' : 'size-5 text-muted-foreground'} aria-hidden="true" /></div>
  {/snippet}
  {#snippet details()}
    <div class="min-w-0 flex-1">
      <p class="truncate text-sm font-medium" title={file.name}>{file.name}</p>
      <p class="truncate text-xs text-muted-foreground" title={new Date(file.created_at).toLocaleString($currentLocale)}>{sizeLabel(file.size)}</p>
    </div>
  {/snippet}
  <div class="flex items-center gap-2">
    {#if previewUrl && onenlarge}
      <button type="button" class="shrink-0" onclick={onenlarge} aria-label={$t('files.enlargeNamed', { name: file.name })}>
        <img src={previewUrl} alt={file.name} class="{thumb} rounded object-cover" />
      </button>
      {@render details()}
    {:else if canPreviewImage(file)}
      {@render icon()}
      {@render details()}
    {:else}
      <button type="button" class="flex min-w-0 flex-1 items-center gap-2 rounded text-left outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={() => preview.open({ device, sessionId, path: file.path })} aria-label={$t('files.previewNamed', { name: file.name })}>
        {@render icon()}
        {@render details()}
      </button>
    {/if}
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
