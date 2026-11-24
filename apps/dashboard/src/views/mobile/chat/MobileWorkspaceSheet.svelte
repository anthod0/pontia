<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { workspaceTitle } from '../../../components/chat/sessionMetadata'
  import type { WorkspaceView } from '../../../api/types'
  let {
    workspaces,
    value,
    allowAll = false,
    close,
    choose,
  }: {
    workspaces: WorkspaceView[]
    value: string
    allowAll?: boolean
    close: () => void
    choose: (id: string) => void
  } = $props()
  let search = $state('')
  function select(id: string) {
    choose(id)
    close()
  }
</script>

<Sheet.Root
  open
  onOpenChange={(open) => {
    if (!open) close()
  }}
>
  <BottomSheet dismiss={close} aria-describedby={undefined}>
    <Sheet.Title class="mb-4 pr-12 text-lg font-semibold">{$t('workspace.title')}</Sheet.Title>
    <input
      type="search"
      class="mb-2 rounded-xl border px-3 py-2 text-base"
      bind:value={search}
      placeholder={$t('common.searchOptions')}
      aria-label={$t('common.searchOptions')}
    />
    <div class="overflow-y-auto">
      {#if allowAll && !search}<button class="mobile-row" aria-pressed={!value} onclick={() => select('')}
          >{$t('workspaces.all')}</button
        >{/if}
      {#each workspaces.filter((w) => (workspaceTitle(w) + ' ' + w.display_path)
          .toLowerCase()
          .includes(search.trim().toLowerCase())) as w (w.workspace_id)}
        <button
          class="mobile-row flex-col items-start! gap-0! aria-pressed:text-primary"
          aria-pressed={value === w.workspace_id}
          onclick={() => select(w.workspace_id)}
        >
          {workspaceTitle(w)}<small class="font-mono text-muted-foreground">{w.display_path}</small>
        </button>
      {/each}
    </div>
  </BottomSheet>
</Sheet.Root>
