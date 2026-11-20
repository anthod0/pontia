<script lang="ts">
  import { t, number } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import type { SharedFile } from '../../api/types'
  import { fileKind } from './fileKind'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import FileXIcon from 'phosphor-svelte/lib/FileXIcon'

  let { file, sessionId }: { file: SharedFile; sessionId: string } = $props()
  const device = useDeviceRuntime()

  const dot = $derived(file.name.lastIndexOf('.'))
  const extension = $derived(dot > 0 ? file.name.slice(dot + 1) : '')
  const stem = $derived(dot > 0 ? file.name.slice(0, dot) : file.name)
  const kind = $derived(fileKind(extension))

  const key = $derived(downloadKey(device.id, sessionId, { kind: 'shared', fileId: file.file_id }))
  const task = $derived($downloads.find(task => task.key === key))
  const receiving = $derived(task?.status === 'receiving')
  const progress = $derived(receiving ? task!.progress : null)
  const percent = $derived(progress?.total ? progress.received / progress.total * 100 : null)
  const failure = $derived(task?.status === 'failed' ? task.error : null)
  const missing = $derived(task?.status === 'failed' && task.errorCode === 'not_found')

  function download() {
    downloads.start(device.id, sessionId, { kind: 'shared', fileId: file.file_id }, file.name)
  }

  function sizeLabel(size: number) {
    return size < 1024 ? `${$number(size)} B` : size < 1024 * 1024
      ? `${$number(size / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(size / 1024 / 1024, { maximumFractionDigits: 1 })} MiB`
  }
</script>

<div class={['flex w-80 max-w-full items-center gap-3 rounded-2xl bg-muted p-2.5', missing && 'opacity-80']} data-bot-file>
  <div class={['grid size-11 shrink-0 place-items-center rounded-xl', missing ? 'bg-background text-muted-foreground' : kind.tone]}>
    {#if missing}<FileXIcon class="size-6" />{:else}<kind.icon class="size-6" weight="duotone" />{/if}
  </div>
  <div class="min-w-0 flex-1">
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
  </div>
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
</div>
