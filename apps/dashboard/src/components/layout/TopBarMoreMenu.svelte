<script lang="ts">
  import type { Snippet } from 'svelte'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { downloads } from '$lib/downloads/manager'
  import { t, number } from '$lib/i18n'

  let { children }: { children?: Snippet } = $props()
  const activeCount = $derived($downloads.filter(task => task.status === 'receiving').length)
</script>

{#if children || $downloads.length}
<div class="pointer-events-auto ml-auto flex h-12 shrink-0 items-center rounded-lg bg-popover px-0.5 ring-1 ring-foreground/10 md:hidden">
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="ghost" size="icon-sm" class="size-11" aria-label={$t('nav.more')}>
          <DotsThreeIcon class="size-6" weight="bold" />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content align="end" class="w-56">
      {@render children?.()}
      {#if $downloads.length}
      <DropdownMenu.Item class="min-h-11 gap-3 px-3" onclick={() => document.dispatchEvent(new CustomEvent('pontia:open-downloads'))}>
        <DownloadSimpleIcon class={activeCount ? 'text-primary' : ''} /> {$t('downloads.title')}
        <Badge data-testid="downloads-count" class="ml-auto h-4 min-w-4 rounded-full px-1 text-[0.625rem] tabular-nums">{$number($downloads.length)}</Badge>
      </DropdownMenu.Item>
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
</div>
{/if}
