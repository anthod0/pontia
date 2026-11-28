<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import { t } from '$lib/i18n';
  import type { Snippet } from 'svelte'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import type { SessionCreationOption, WorkspaceView } from '../../api/types'
  import { workspaceTitle } from './sessionMetadata'
  import ClientIcon from './ClientIcon.svelte'
  import { registerShortcut } from '$lib/shortcuts/dispatch'
  import { adjacentValue } from '$lib/shortcuts/sessionNavigation'
  import { targetLabelClass, targetSegmentClass, targetToolbarClass } from './targetSegment'
  import { useAgentClientOnboardingStatus } from '../../features/onboarding/context.svelte'

  interface Props {
    workspaceId: string
    creationOptionId: string
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    creationOptions: SessionCreationOption[]
    fixedWorkspace?: boolean
    deviceSelector?: Snippet<[boolean]>
    children?: Snippet
    disabled?: boolean
  }

  let {
    workspaceId = $bindable(''),
    creationOptionId = $bindable(''),
    workspaces,
    workspacesLoading = false,
    selectedWorkspace,
    creationOptions,
    fixedWorkspace = false,
    deviceSelector,
    children,
    disabled = false,
  }: Props = $props()

  const selectedOption = $derived(creationOptions.find((option) => option.id === creationOptionId))
  const onboarding = useAgentClientOnboardingStatus()
  const clientSelectDisabled = $derived(!fixedWorkspace && (disabled || onboarding?.probing || !creationOptions.length))
  const workspaceSelectDisabled = $derived(fixedWorkspace || disabled || workspacesLoading || !workspaces.length)

  registerShortcut('switchAgentClient', event => {
    if (!clientSelectDisabled) creationOptionId = adjacentValue(creationOptions.map(option => option.id), creationOptionId, event.shiftKey ? -1 : 1) ?? creationOptionId
  })
  registerShortcut('switchWorkspace', event => {
    if (!workspaceSelectDisabled) workspaceId = adjacentValue(workspaces.map(workspace => workspace.workspace_id), workspaceId, event.shiftKey ? -1 : 1) ?? workspaceId
  })
</script>

{#if fixedWorkspace}
  <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 px-1 text-sm text-muted-foreground sm:text-base">
    <span>{$t('chat.startInPrefix')}</span>
    <span class="inline-flex h-7 max-w-64 items-center gap-1.5 px-1.5 text-sm font-medium text-foreground sm:text-base" title={selectedWorkspace?.canonical_path}>
      <FolderIcon class="size-4 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 truncate">{selectedWorkspace ? workspaceTitle(selectedWorkspace) : $t('workspace.title')}</span>
    </span>
    <span>{$t('chat.useClientPrefix')}</span>
    <ResponsiveSelect bind:value={creationOptionId}
      label={$t('clients.client')} triggerClass="h-7 max-w-44 border-0 bg-transparent px-1.5 font-medium text-foreground hover:bg-muted" side="top" align="start"
      options={creationOptions} keyOf={option => option.id} labelOf={option => option.name}>
      {#snippet trigger()}
        <ClientIcon clientType={selectedOption?.client_type} clientKey={selectedOption?.client_key} class="size-4 text-foreground" />
        <span class="min-w-0 truncate">{selectedOption?.name ?? $t('clients.choose')}</span>
      {/snippet}
      {#snippet option(option)}
        <ClientIcon clientType={option.client_type} clientKey={option.client_key} class="size-3.5 text-foreground" />
        {option.name}
      {/snippet}
    </ResponsiveSelect>
  </div>
{:else}
  <div class={targetToolbarClass}>
    {@render deviceSelector?.(disabled)}
    <ResponsiveSelect bind:value={creationOptionId} disabled={clientSelectDisabled}
      label={$t('clients.agentClient')} triggerClass={targetSegmentClass} side="top" align="start"
      options={creationOptions} keyOf={option => option.id} labelOf={option => option.name}>
      {#snippet trigger()}
        <ClientIcon clientType={selectedOption?.client_type} clientKey={selectedOption?.client_key} class="size-3.5 text-foreground" />
        <span class={targetLabelClass}>{onboarding?.probing ? $t('onboarding.clients.probing') : selectedOption?.name ?? $t('clients.agentClient')}</span>
      {/snippet}
      {#snippet option(option)}
        <ClientIcon clientType={option.client_type} clientKey={option.client_key} class="size-3.5 text-foreground" />
        {option.name}
      {/snippet}
    </ResponsiveSelect>
    <ResponsiveSelect bind:value={workspaceId} disabled={workspaceSelectDisabled}
      label={$t('workspace.title')} triggerClass={targetSegmentClass} title={selectedWorkspace?.canonical_path} align="start" contentClass="w-72" itemClass="py-2"
      options={workspaces} keyOf={workspace => workspace.workspace_id} labelOf={workspace => workspaceTitle(workspace)}>
      {#snippet trigger()}
        <FolderIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
        <span class={targetLabelClass}>{selectedWorkspace ? workspaceTitle(selectedWorkspace) : workspacesLoading ? $t('common.loading') : $t('workspace.title')}</span>
      {/snippet}
      {#snippet option(workspace)}
        <div class="flex min-w-0 flex-col">
          <span class="truncate">{workspaceTitle(workspace)}</span>
          <span class="truncate font-mono text-11 text-muted-foreground">{workspace.display_path}</span>
        </div>
      {/snippet}
    </ResponsiveSelect>
    {@render children?.()}
  </div>
{/if}
