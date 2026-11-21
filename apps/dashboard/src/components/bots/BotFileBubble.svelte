<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t, number } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { canPreviewImage } from '$lib/downloads/preview'
  import { ApiError } from '../../api/errors'
  import type { SharedFile } from '../../api/types'
  import { useFilePreview } from '../chat/filePreviewContext'
  import SharedImageViewer from '../chat/SharedImageViewer.svelte'
  import { fileKind } from '../chat/fileKind'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import FileXIcon from 'phosphor-svelte/lib/FileXIcon'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'

  let { file, sessionId }: { file: SharedFile; sessionId: string } = $props()
  const device = useDeviceRuntime()
  const preview = useFilePreview()

  const dot = $derived(file.name.lastIndexOf('.'))
  const extension = $derived(dot > 0 ? file.name.slice(dot + 1) : '')
  const stem = $derived(dot > 0 ? file.name.slice(0, dot) : file.name)
  const kind = $derived(fileKind(extension))

  const key = $derived(downloadKey(device.id, sessionId, { kind: 'shared', fileId: file.file_id }))
  const task = $derived($downloads.find(task => task.key === key))
  const receiving = $derived(task?.status === 'receiving')
  const progress = $derived(receiving ? task!.progress : null)
  const percent = $derived(progress?.total ? progress.received / progress.total * 100 : null)
  // Loading an image preview reads the same file, so its failure also tells whether the file is gone.
  let previewFailure = $state<ApiError | Error | null>(null)
  const previewMissing = $derived(previewFailure instanceof ApiError && previewFailure.code === 'not_found')
  // A download started after a failed preview supersedes the preview's error.
  const failure = $derived(task ? (task.status === 'failed' ? task.error : null) : previewFailure ? (previewMissing ? $t('files.missing') : previewFailure.message) : null)
  const missing = $derived(task ? task.status === 'failed' && task.errorCode === 'not_found' : previewMissing)

  const image = $derived(canPreviewImage(file))
  let imageUrl = $state<string | null | undefined>(undefined)
  let viewerOpen = $state(false)
  let destroyed = false
  onDestroy(() => { destroyed = true })
  const showImage = $derived(image && imageUrl !== null && !previewFailure && !missing)

  function observeImage(node: HTMLElement) {
    if (!image) return
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return
      observer.disconnect()
      void device.previews.sharedImageUrl(sessionId, file.file_id).then(url => {
        if (!destroyed) imageUrl = url
      }).catch(cause => {
        if (!destroyed) previewFailure = cause instanceof Error ? cause : new Error(String(cause))
      })
    })
    observer.observe(node)
    return { destroy: () => observer.disconnect() }
  }

  function download() {
    downloads.start(device.id, sessionId, { kind: 'shared', fileId: file.file_id }, file.name)
  }

  function sizeLabel(size: number) {
    return size < 1024 ? `${$number(size)} B` : size < 1024 * 1024
      ? `${$number(size / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(size / 1024 / 1024, { maximumFractionDigits: 1 })} MiB`
  }
</script>

{#snippet details()}
  <p class={['flex min-w-0 text-sm font-medium text-heading', missing && 'text-muted-foreground line-through decoration-muted-foreground/50']} title={file.name}>
    <span class="truncate">{stem}</span>{#if extension}<span class="shrink-0">.{extension}</span>{/if}
  </p>
  {#if progress}
    <p class="truncate text-xs text-muted-foreground tabular-nums" role="status">
      {$t('files.receiving')} {progress.total === null ? sizeLabel(progress.received) : `${sizeLabel(progress.received)} / ${sizeLabel(progress.total)}`}
    </p>
  {:else if failure}
    <p class={['text-xs [overflow-wrap:anywhere]', missing ? 'text-muted-foreground' : 'text-destructive']} role="alert">{failure}</p>
  {:else}
    <p class="truncate text-xs text-muted-foreground tabular-nums">{extension ? extension.toUpperCase() : $t('files.file')} · {sizeLabel(file.size)}</p>
  {/if}
{/snippet}

{#snippet action()}
  {#if receiving}
    <button type="button" class="relative grid size-9 shrink-0 place-items-center rounded-full text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" onclick={() => downloads.cancel(task!.id)} aria-label={$t('downloads.cancelNamed', { name: file.name })} title={$t('common.cancel')}>
      <svg viewBox="0 0 36 36" class="absolute inset-0 size-9 -rotate-90" aria-hidden="true">
        <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" class="stroke-border" />
        {#if percent === null}
          <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class="origin-center animate-spin stroke-primary" stroke-dasharray="25 100" />
        {:else}
          <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class="stroke-primary transition-[stroke-dashoffset]" pathLength="100" stroke-dasharray="100" stroke-dashoffset={100 - percent} />
        {/if}
      </svg>
      <XIcon class="size-3.5" weight="bold" />
    </button>
  {:else if !missing}
    <button type="button" class="grid size-9 shrink-0 place-items-center rounded-full bg-background text-foreground ring-1 ring-border outline-none transition-colors hover:bg-primary-solid hover:text-primary-foreground hover:ring-primary-solid focus-visible:ring-2 focus-visible:ring-ring" onclick={download} aria-label={failure ? $t('downloads.retryNamed', { name: file.name }) : $t('files.downloadNamed', { name: file.name })} title={failure ? $t('common.retry') : $t('files.download')}>
      {#if failure}<ArrowClockwiseIcon class="size-4" />{:else}<DownloadSimpleIcon class="size-4" weight="bold" />{/if}
    </button>
  {/if}
{/snippet}

<div use:observeImage class="w-80 max-w-full" data-bot-file>
  {#if showImage}
    <figure class="overflow-hidden rounded-2xl bg-muted">
      {#if imageUrl}
        <button type="button" class="block w-full outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset" onclick={() => { viewerOpen = true }} aria-label={$t('files.enlargeNamed', { name: file.name })}>
          <img src={imageUrl} alt={file.name} class="max-h-72 w-full object-cover" />
        </button>
      {:else}
        <Skeleton class="aspect-[16/10] w-full rounded-none" role="status" aria-label={$t('files.loadingPreview')} />
      {/if}
      <figcaption class="flex items-center gap-3 px-3 py-2">
        <div class="min-w-0 flex-1">{@render details()}</div>
        {@render action()}
      </figcaption>
    </figure>
    <SharedImageViewer {sessionId} files={[file]} bind:open={viewerOpen} />
  {:else}
    <div class={['flex items-center gap-2 rounded-2xl bg-muted p-2.5', missing && 'opacity-80']}>
      {#snippet summary()}
        <div class={['grid size-11 shrink-0 place-items-center rounded-xl', missing ? 'bg-background text-muted-foreground' : kind.tone]}>
          {#if missing}<FileXIcon class="size-6" />{:else}<kind.icon class="size-6" weight="duotone" />{/if}
        </div>
        <div class="min-w-0 flex-1">{@render details()}</div>
      {/snippet}
      {#if missing || image || !kind.text}
        <div class="flex min-w-0 flex-1 items-center gap-3">{@render summary()}</div>
      {:else}
        <button type="button" class="-m-1 flex min-w-0 flex-1 items-center gap-3 rounded-xl p-1 text-left outline-none hover:bg-background/60 focus-visible:ring-2 focus-visible:ring-ring" onclick={() => preview.open({ device, sessionId, path: file.path })} aria-label={$t('files.previewNamed', { name: file.name })}>
          {@render summary()}
        </button>
      {/if}
      {@render action()}
    </div>
  {/if}
</div>
