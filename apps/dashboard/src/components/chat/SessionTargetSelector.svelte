<script lang="ts">
  import type { Snippet } from 'svelte'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { SessionCreationOption, WorkspaceView } from '../../api/types'
  import { workspaceTitle } from './sessionMetadata'
  import ClientIcon from './ClientIcon.svelte'

  interface Props {
    workspaceId: string
    creationOptionId: string
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    creationOptions: SessionCreationOption[]
    fixedWorkspace?: boolean
    deviceSelector?: Snippet<[boolean]>
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
    disabled = false,
  }: Props = $props()

  const selectedOption = $derived(creationOptions.find((option) => option.id === creationOptionId))
</script>

{#if fixedWorkspace}
  <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 px-1 text-sm text-muted-foreground sm:text-base">
    <span>Start a new agent session in</span>
    <span class="inline-flex h-7 max-w-64 items-center gap-1.5 px-1.5 text-sm font-medium text-foreground sm:text-base" title={selectedWorkspace?.canonical_path}>
      <FolderIcon class="size-4 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 truncate">{selectedWorkspace ? workspaceTitle(selectedWorkspace) : 'Workspace'}</span>
    </span>
    <span>, use</span>
    <Select.Root type="single" bind:value={creationOptionId}>
      <Select.Trigger class="h-7 max-w-44 border-0 bg-transparent px-1.5 font-medium text-foreground hover:bg-muted" aria-label="Client">
        <ClientIcon clientType={selectedOption?.client_type} acpAgentId={selectedOption?.metadata.acp_agent_id} class="size-4 text-foreground" />
        <span class="min-w-0 truncate">{selectedOption?.name ?? 'Choose an agent'}</span>
      </Select.Trigger>
      <Select.Content side="top" align="start">
        {#each creationOptions as option (option.id)}
          <Select.Item value={option.id} label={option.name}>
            <ClientIcon clientType={option.client_type} acpAgentId={option.metadata.acp_agent_id} class="size-3.5 text-foreground" />
            {option.name}
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
{:else}
  <div class="flex min-w-0 items-center gap-2">
    {@render deviceSelector?.(disabled)}
    <Select.Root type="single" bind:value={creationOptionId} disabled={disabled || !creationOptions.length}>
      <Select.Trigger class="h-[34px] max-w-48 shrink-0 gap-1.5 bg-background px-3 text-[13px] font-medium text-heading hover:bg-muted data-[state=open]:border-primary data-[state=open]:bg-selected" aria-label="Agent client">
        <ClientIcon clientType={selectedOption?.client_type} acpAgentId={selectedOption?.metadata.acp_agent_id} class="size-3.5 text-foreground" />
        <span class="min-w-0 truncate">{selectedOption?.name ?? 'Agent client'}</span>
      </Select.Trigger>
      <Select.Content side="top" align="start">
        {#each creationOptions as option (option.id)}
          <Select.Item value={option.id} label={option.name}>
            <ClientIcon clientType={option.client_type} acpAgentId={option.metadata.acp_agent_id} class="size-3.5 text-foreground" />
            {option.name}
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
    <Select.Root type="single" bind:value={workspaceId} disabled={disabled || workspacesLoading || !workspaces.length}>
      <Select.Trigger class="h-[34px] max-w-48 shrink-0 gap-1.5 bg-background px-3 text-[13px] font-medium text-heading hover:bg-muted data-[state=open]:border-primary data-[state=open]:bg-selected" aria-label="Workspace" title={selectedWorkspace?.canonical_path}>
        <FolderIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
        <span class="min-w-0 truncate">{selectedWorkspace ? workspaceTitle(selectedWorkspace) : workspacesLoading ? 'Loading…' : 'Workspace'}</span>
      </Select.Trigger>
      <Select.Content align="start" class="w-72">
        {#each workspaces as workspace (workspace.workspace_id)}
          <Select.Item value={workspace.workspace_id} label={workspaceTitle(workspace)} class="py-2">
            <div class="flex min-w-0 flex-col">
              <span class="truncate">{workspaceTitle(workspace)}</span>
              <span class="truncate font-mono text-[11px] text-muted-foreground">{workspace.display_path}</span>
            </div>
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
{/if}
