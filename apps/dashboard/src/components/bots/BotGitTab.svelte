<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import FolderSimpleIcon from 'phosphor-svelte/lib/FolderSimpleIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import GearIcon from 'phosphor-svelte/lib/GearIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { createGitWorkspace } from '$lib/botGitWorkspace'
  import type { WorkspaceView } from '../../api/types'
  import { useDeviceRuntime } from '../../device/context'
  import GitPanel from '../chat/git/GitPanel.svelte'
  import { GitPanelState } from '../chat/git/gitPanelState.svelte'
  import { workspaceTitle } from '../chat/sessionMetadata'

  /** The Git tab of a Bot chat's side panel: a Workspace picker over that Workspace's changes and history. Mounted only while the tab shows. */
  let { botId, writable = false }: { botId: string; writable?: boolean } = $props()

  const device = useDeviceRuntime()
  // The panel lives as long as its Bot chat page, which is recreated for another Bot.
  // svelte-ignore state_referenced_locally
  const saved = createGitWorkspace(device.id, botId)
  const workspacesQuery = device.workspaceQueries.createWorkspacesQuery()
  const workspaces = $derived(workspacesQuery.data ?? [])
  // A saved Workspace that is no longer registered, or cannot be read, counts as none chosen.
  const workspace = $derived(workspaces.find(item => item.workspace_id === saved.current && !unavailable(item)) ?? null)
  const git = new GitPanelState()
  let view = $state<GitPanel>()

  function choose(workspaceId: string) {
    if (workspaceId === saved.current) return
    saved.current = workspaceId
    git.reset()
  }

  function unavailable(item: WorkspaceView): boolean {
    return item.availability.status === 'unavailable'
  }

  // Branch and upstream come from the status projection, which no Session keeps fresh on this page.
  $effect(() => {
    if (workspace) void device.workspaceQueries.refreshWorkspaceGitStatus(workspace.workspace_id).catch(() => {})
  })
</script>

{#snippet workspaceLabel(item: WorkspaceView)}
  <div class="flex min-w-0 flex-1 flex-col text-left">
    <span class="truncate">{workspaceTitle(item)}</span>
    <span class="truncate font-mono text-11 text-muted-foreground">{item.display_path}</span>
  </div>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex min-h-0 flex-1 flex-col" onkeydown={(event) => view?.keydown(event)}>
  {#if workspace}
    {#key `${device.id}\0${workspace.workspace_id}`}
      <GitPanel bind:this={view} {writable} {device} workspaceId={workspace.workspace_id} {git}>
        {#snippet workspaceHeading()}
            <ResponsiveSelect value={workspace.workspace_id} onValueChange={choose}
              label={$t('bots.gitWorkspace', { name: workspaceTitle(workspace) })} triggerClass="h-7 min-w-0 max-w-full rounded-chip border-0 px-2 shadow-none data-[size=default]:h-7 hover:bg-muted" title={workspace.canonical_path} align="start" contentClass="w-72" itemClass="py-2" disabledOption={unavailable}
              options={workspaces} keyOf={item => item.workspace_id} labelOf={item => workspaceTitle(item)}>
              {#snippet trigger()}
                <FolderIcon class="size-3.5 text-muted-foreground" />
                <span class="min-w-0 flex-1 truncate text-left text-13 font-medium text-heading">{workspaceTitle(workspace)}</span>
                <CaretDownIcon class="size-3 shrink-0 text-muted-foreground" />
              {/snippet}
              {#snippet option(item)}{@render workspaceLabel(item)}{/snippet}
            </ResponsiveSelect>
        {/snippet}
      </GitPanel>
    {/key}
  {:else if workspacesQuery.isError}
    <div role="alert" class="p-3 text-sm">{$t('workspaces.loadFailedMessage')} <Button variant="ghost" size="xs" onclick={() => workspacesQuery.refetch()}>{$t('common.retry')}</Button></div>
  {:else if !workspacesQuery.data}
    <div role="status" aria-label={$t('common.loading')} class="space-y-2 p-3">
      {#each [0, 1, 2] as index (index)}<Skeleton class="h-14 rounded-item" />{/each}
    </div>
  {:else if !workspaces.length}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-item bg-muted text-muted-foreground"><FolderIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('workspaces.empty')}</Empty.Title>
        <Empty.Description>{$t('bots.gitNoWorkspaces')}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button variant="outline" size="sm" onclick={() => navigate('/settings/workspaces')}><GearIcon data-icon="inline-start" />{$t('bots.gitManageWorkspaces')}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else}
    <section class="min-h-0 flex-1 overflow-auto overscroll-contain p-3" aria-labelledby="bot-git-workspaces">
      <h2 id="bot-git-workspaces" class="px-2 text-sm font-medium text-heading">{$t('bots.gitChooseWorkspace')}</h2>
      <p class="px-2 pt-0.5 pb-2 text-xs text-muted-foreground">{$t('bots.gitChooseWorkspaceDescription')}</p>
      <ul>
        {#each workspaces as item (item.workspace_id)}
          <li>
            <button type="button" disabled={unavailable(item)} title={item.canonical_path} onclick={() => choose(item.workspace_id)}
              class="flex w-full items-center gap-3 rounded-item p-2 text-left text-sm outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50">
              <span class="grid size-9 shrink-0 place-items-center rounded-item bg-muted text-muted-foreground"><FolderSimpleIcon class="size-5" weight="duotone" /></span>
              {@render workspaceLabel(item)}
              {#if unavailable(item)}<span class="ml-auto flex shrink-0 items-center gap-1 text-xs text-muted-foreground"><WarningCircleIcon class="size-3.5" />{$t('common.unavailable')}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>
