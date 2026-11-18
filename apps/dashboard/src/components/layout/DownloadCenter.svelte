<script lang="ts">
  import { onMount } from 'svelte'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XCircleIcon from 'phosphor-svelte/lib/XCircleIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { downloads } from '$lib/downloads/manager'
  import { t, number } from '$lib/i18n'

  let open = $state(false)
  onMount(() => {
    const openDownloads = () => { open = true }
    document.addEventListener('pontia:open-downloads', openDownloads)
    return () => document.removeEventListener('pontia:open-downloads', openDownloads)
  })
  const activeCount = $derived($downloads.filter(task => task.status === 'receiving').length)
  const hasFinished = $derived($downloads.some(task => task.status !== 'receiving'))

  function sizeLabel(size: number) {
    return size < 1024 ? `${$number(size)} B` : size < 1024 * 1024
      ? `${$number(size / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(size / (1024 * 1024), { maximumFractionDigits: 1 })} MiB`
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="ghost" size="sm" class="pointer-events-auto hidden md:inline-flex h-10 gap-1.5 bg-popover px-2.5 ring-1 ring-foreground/10 md:h-8 md:px-2 [&_svg:not([class*=size-])]:size-5 md:[&_svg:not([class*=size-])]:size-3.5" aria-label={$t('downloads.title')} title={$t('downloads.title')}>
        <DownloadSimpleIcon class={activeCount ? 'text-primary' : ''} />
        {#if activeCount}<span class="text-xs tabular-nums">{$number(activeCount)}</span>{/if}
        <span class="sr-only" role="status">{activeCount ? $t('downloads.activeCount', { count: activeCount }) : ''}</span>
      </Button>
    {/snippet}
  </Dialog.Trigger>
  <Dialog.Content class="w-[calc(100%_-_2rem)] gap-0 overflow-hidden p-0 sm:w-full sm:max-w-lg">
    <Dialog.Header class="gap-1 border-b px-5 py-4 pr-12">
      <Dialog.Title>{$t('downloads.title')}</Dialog.Title>
    </Dialog.Header>
    {#if $downloads.length}
      <ul class="max-h-[min(60svh,28rem)] divide-y overflow-y-auto px-5" aria-label={$t('downloads.title')}>
        {#each $downloads as task (task.id)}
          <li class="py-4">
            <div class="flex items-start gap-3">
              <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
                <FileIcon class="size-4" />
              </div>
              <div class="min-w-0 flex-1 space-y-1">
                <p class="truncate text-sm font-medium" title={task.name}>{task.name}</p>
                <p class="truncate text-xs text-muted-foreground">{task.deviceName ?? $t('downloads.localDevice')}</p>
              </div>
              {#if task.status === 'receiving'}
                <Button variant="ghost" size="icon-sm" onclick={() => downloads.cancel(task.id)} aria-label={$t('downloads.cancelNamed', { name: task.name })} title={$t('common.cancel')}><XIcon /></Button>
              {:else if task.status === 'failed'}
                <Button variant="ghost" size="icon-sm" onclick={() => downloads.retry(task.id)} aria-label={$t('downloads.retryNamed', { name: task.name })} title={$t('common.retry')}><ArrowClockwiseIcon /></Button>
              {/if}
            </div>
            <div class="mt-3 space-y-2 pl-12">
              {#if task.status === 'receiving'}
                <Progress value={task.progress.total === null ? null : task.progress.total === 0 ? 100 : task.progress.received / task.progress.total * 100} aria-label={$t('files.receivingNamed', { name: task.name })} />
                <div class="flex justify-between gap-2 text-xs tabular-nums text-muted-foreground">
                  <span>{$t('files.receiving')}</span>
                  <span>{sizeLabel(task.progress.received)}{task.progress.total === null ? '' : ` / ${sizeLabel(task.progress.total)}`}</span>
                </div>
              {:else}
                <p class={['flex items-center gap-1.5 text-xs', task.status === 'failed' ? 'text-destructive' : 'text-muted-foreground']}>
                  {#if task.status === 'complete'}
                    <CheckCircleIcon class="size-3.5 text-primary" />
                    <span>{sizeLabel(task.progress.received)}</span>
                  {:else if task.status === 'failed'}
                    <WarningCircleIcon class="size-3.5 shrink-0" />
                    <span>{$t('downloads.failed')}</span>
                  {:else}
                    <XCircleIcon class="size-3.5" />
                    <span>{$t('downloads.cancelled')}</span>
                  {/if}
                </p>
                {#if task.error}<p class="break-words text-xs text-destructive">{task.error}</p>{/if}
              {/if}
            </div>
          </li>
        {/each}
      </ul>
      <div class="flex items-center justify-between gap-3 border-t px-5 py-3">
        <p class="text-xs text-muted-foreground" role="status">{activeCount ? $t('downloads.activeCount', { count: activeCount }) : $t('downloads.noActive')}</p>
        <Button variant="ghost" size="sm" disabled={!hasFinished} onclick={() => downloads.clearFinished()}>{$t('downloads.clearFinished')}</Button>
      </div>
    {:else}
      <div class="flex flex-col items-center gap-3 px-5 py-10 text-muted-foreground">
        <DownloadSimpleIcon class="size-7" />
        <p class="text-sm">{$t('downloads.empty')}</p>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
