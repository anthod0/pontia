<script lang="ts">
  import { Folder, Terminal } from '@lucide/svelte'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { WorkspaceView } from '../../api/types'
  import { clientTitle, workspaceTitle } from './sessionMetadata'

  interface Props {
    workspaceId: string
    clientType: string
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    clientTypeOptions: string[]
    fixedWorkspace?: boolean
  }

  let {
    workspaceId = $bindable(''),
    clientType = $bindable('pi'),
    workspaces,
    workspacesLoading = false,
    selectedWorkspace,
    clientTypeOptions,
    fixedWorkspace = false,
  }: Props = $props()
</script>

<div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 px-1 text-sm text-muted-foreground sm:text-base">
  <span>Start a new agent session {fixedWorkspace ? 'in' : 'from'}</span>
  {#if fixedWorkspace}
    <span class="inline-flex h-7 max-w-64 items-center gap-1.5 rounded-md px-1.5 text-sm font-medium text-foreground sm:text-base" title={selectedWorkspace?.canonical_path ?? undefined}>
      <Folder class="size-4 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 truncate">{#if selectedWorkspace}{workspaceTitle(selectedWorkspace)}{:else}Workspace{/if}</span>
    </span>
  {:else}
    <Select.Root type="single" bind:value={workspaceId} disabled={workspacesLoading}>
      <Select.Trigger class="h-7 max-w-64 rounded-md border-0 bg-transparent px-1.5 text-sm font-medium text-foreground shadow-none hover:bg-muted focus:ring-1 sm:text-base" aria-label="Workspace" title={selectedWorkspace?.canonical_path ?? undefined}>
        <Folder class="size-4 text-muted-foreground" aria-hidden="true" />
        <span class="min-w-0 truncate">{#if selectedWorkspace}{workspaceTitle(selectedWorkspace)}{:else}Workspace{/if}</span>
      </Select.Trigger>
      <Select.Content align="start">
        {#each workspaces as workspace (workspace.workspace_id)}
          <Select.Item value={workspace.workspace_id} label={workspaceTitle(workspace)}>
            <div class="flex min-w-0 flex-col">
              <span class="truncate">{workspaceTitle(workspace)}</span>
              <span class="truncate text-xs text-muted-foreground">{workspace.display_path}</span>
            </div>
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  {/if}

  <span>, use</span>
  <Select.Root type="single" bind:value={clientType}>
    <Select.Trigger class="h-7 max-w-44 rounded-md border-0 bg-transparent px-1.5 text-sm font-medium text-foreground shadow-none hover:bg-muted focus:ring-1 sm:text-base" aria-label="Client">
      <Terminal class="size-4 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 truncate">{clientTitle(clientType)}</span>
    </Select.Trigger>
    <Select.Content align="start">
      {#each clientTypeOptions as option (option)}
        <Select.Item value={option} label={option}>{option}</Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>
</div>
