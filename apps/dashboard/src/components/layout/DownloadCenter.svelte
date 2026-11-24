<script lang="ts">
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { downloads } from '$lib/downloads/manager'
  import { t, number } from '$lib/i18n'

  import DownloadList from './DownloadList.svelte'
  let open = $state(false)
  const activeCount = $derived($downloads.filter(task => task.status === 'receiving').length)
  // Finished downloads leave the list, so the dialog closes with its last entry.
  $effect(() => {
    if (!$downloads.length) open = false
  })

</script>

<Dialog.Root bind:open>
  {#if $downloads.length}
  <Dialog.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="ghost" size="sm" class="pointer-events-auto relative hidden md:inline-flex h-10 gap-1.5 bg-popover px-2.5 ring-1 ring-foreground/10 md:h-8 md:px-2 [&_svg:not([class*=size-])]:size-5 md:[&_svg:not([class*=size-])]:size-3.5" aria-label={$t('downloads.title')} title={$t('downloads.title')}>
        <DownloadSimpleIcon class={activeCount ? 'text-primary' : ''} />
        <Badge data-testid="downloads-count" class="absolute -top-1.5 -right-1.5 h-4 min-w-4 rounded-full px-1 text-[0.625rem] tabular-nums">{$number($downloads.length)}</Badge>
        <span class="sr-only" role="status">{activeCount ? $t('downloads.activeCount', { count: activeCount }) : ''}</span>
      </Button>
    {/snippet}
  </Dialog.Trigger>
  {/if}
  <Dialog.Content class="w-[calc(100%_-_2rem)] gap-0 overflow-hidden p-0 sm:w-full sm:max-w-lg">
    <Dialog.Header class="gap-1 border-b px-5 py-4 pr-12">
      <Dialog.Title>{$t('downloads.title')}</Dialog.Title>
    </Dialog.Header>
    <DownloadList />
  </Dialog.Content>
</Dialog.Root>
