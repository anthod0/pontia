<script lang="ts">
  import * as Tabs from '$lib/components/ui/tabs'
  import { t } from '$lib/i18n'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import FolderOpenIcon from 'phosphor-svelte/lib/FolderOpenIcon'

  let { workspace, open }: { workspace: string; open: () => void } = $props()
  const filters = ['active', 'all', 'pinned', 'archived'] as const
</script>

<div class="sticky top-0 z-1 flex h-11 items-stretch justify-between gap-2 border-b bg-surface">
  <Tabs.List variant="line" aria-label={$t('sessions.views')} class="h-full max-w-[calc(100%-3.5rem)] shrink-0 gap-5 p-0 max-[360px]:gap-2">
    {#each filters as filter (filter)}
      <Tabs.Trigger
        value={filter}
        class="h-full min-w-0 flex-initial px-0 text-[15px] text-muted-foreground aria-selected:font-semibold aria-selected:text-heading after:bg-primary group-data-[orientation=horizontal]/tabs:after:-bottom-px"
      ><span class="truncate">{$t(`common.${filter}`)}</span></Tabs.Trigger>
    {/each}
  </Tabs.List>
  <button
    class="flex min-w-12 max-w-33 items-center gap-1 text-[13px] {workspace ? 'font-medium text-primary' : 'text-muted-foreground'}"
    aria-haspopup="dialog"
    title={workspace || $t('workspaces.all')}
    onclick={open}
  >
    {#if workspace}<FolderOpenIcon class="size-4 shrink-0" />{:else}<FolderIcon class="size-4 shrink-0" />{/if}
    <span class="truncate">{workspace || $t('workspaces.all')}</span>
    <CaretDownIcon class="size-3 shrink-0" />
  </button>
</div>
