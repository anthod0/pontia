<script lang="ts">
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import { downloads } from '$lib/downloads/manager'
  import { t, number } from '$lib/i18n'
  function sizeLabel(size: number) {
    return size < 1024 ? `${$number(size)} B` : size < 1024 * 1024
      ? `${$number(size / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(size / (1024 * 1024), { maximumFractionDigits: 1 })} MiB`
  }
</script>
    {#if $downloads.length}
      <ul class="max-h-[min(60svh,28rem)] divide-y overflow-y-auto overscroll-contain px-5" aria-label={$t('downloads.title')}>
        {#each $downloads as task (task.id)}
          <li class="py-4">
            <div class="flex items-start gap-3">
              <div class="flex size-9 shrink-0 items-center justify-center rounded-item bg-muted text-muted-foreground">
                <FileIcon class="size-4" />
              </div>
              <div class="min-w-0 flex-1 space-y-1">
                <p class="truncate text-sm font-medium" title={task.name}>{task.name}</p>
                <p class="truncate text-xs text-muted-foreground">{task.deviceName ?? $t('downloads.localDevice')}</p>
              </div>
              {#if task.status === 'receiving'}
                <Button variant="ghost" size="icon-sm" class="touch-target:size-11" onclick={() => downloads.cancel(task.id)} aria-label={$t('downloads.cancelNamed', { name: task.name })} title={$t('common.cancel')}><XIcon /></Button>
              {:else}
                <div class="flex shrink-0 gap-1">
                  <Button variant="ghost" size="icon-sm" class="touch-target:size-11" onclick={() => downloads.retry(task.id)} aria-label={$t('downloads.retryNamed', { name: task.name })} title={$t('common.retry')}><ArrowClockwiseIcon /></Button>
                  <Button variant="ghost" size="icon-sm" class="touch-target:size-11" onclick={() => downloads.cancel(task.id)} aria-label={$t('downloads.dismissNamed', { name: task.name })} title={$t('common.dismiss')}><XIcon /></Button>
                </div>
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
                <p class="flex items-center gap-1.5 text-xs text-destructive">
                  <WarningCircleIcon class="size-3.5 shrink-0" />
                  <span>{$t('downloads.failed')}</span>
                </p>
                {#if task.error}<p class="break-words text-xs text-destructive">{task.error}</p>{/if}
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
