<script lang="ts">
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { SessionCreationOption, WorkspaceView } from '../../api/types'
  import { workspaceTitle } from './sessionMetadata'

  interface Props {
    workspaceId: string
    creationOptionId: string
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    creationOptions: SessionCreationOption[]
    fixedWorkspace?: boolean
  }

  let {
    workspaceId = $bindable(''),
    creationOptionId = $bindable(''),
    workspaces,
    workspacesLoading = false,
    selectedWorkspace,
    creationOptions,
    fixedWorkspace = false,
  }: Props = $props()
</script>

{#if fixedWorkspace}
  <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 px-1 text-sm text-muted-foreground sm:text-base">
    <span>Start a new agent session in</span>
    <span class="inline-flex h-7 max-w-64 items-center gap-1.5 px-1.5 text-sm font-medium text-foreground sm:text-base" title={selectedWorkspace?.canonical_path}>
      <FolderIcon class="size-4 text-repository" aria-hidden="true" />
      <span class="min-w-0 truncate">{selectedWorkspace ? workspaceTitle(selectedWorkspace) : 'Workspace'}</span>
    </span>
    <span>, use</span>
    <Select.Root type="single" bind:value={creationOptionId}>
      <Select.Trigger class="h-7 max-w-44 border-0 bg-transparent px-1.5 font-medium text-foreground hover:bg-muted" aria-label="Client">
        <TerminalWindowIcon class="size-4 text-primary" aria-hidden="true" />
        <span class="min-w-0 truncate">{creationOptions.find((option) => option.id === creationOptionId)?.name ?? 'Choose an agent'}</span>
      </Select.Trigger>
      <Select.Content align="start">
        {#each creationOptions as option (option.id)}
          <Select.Item value={option.id} label={option.name}>{option.name}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
{:else}
  <div class="space-y-3">
    <div role="group" aria-labelledby="new-chat-client-label">
      <div id="new-chat-client-label" class="mb-2 font-mono text-[11px] tracking-[0.06em] text-muted-foreground uppercase">Agent client</div>
      <div class="flex flex-wrap gap-2">
        {#each creationOptions as option (option.id)}
          <Button
            variant="outline"
            class={`h-[34px] gap-1.5 px-3 text-[13px] ${creationOptionId === option.id ? 'border-primary bg-selected' : ''}`}
            aria-pressed={creationOptionId === option.id}
            onclick={() => (creationOptionId = option.id)}
          >
            <TerminalWindowIcon class="size-3.5 text-primary" aria-hidden="true" />
            {option.name}
          </Button>
        {/each}
      </div>
    </div>
    <div>
      <div id="new-chat-workspace-label" class="mb-2 font-mono text-[11px] tracking-[0.06em] text-muted-foreground uppercase">Workspace</div>
      <Select.Root type="single" bind:value={workspaceId} disabled={workspacesLoading || !workspaces.length}>
        <Select.Trigger class="h-auto min-h-11 w-full gap-2.5 bg-background px-3 py-2.5 text-left hover:bg-muted data-[state=open]:border-primary data-[state=open]:bg-selected" aria-labelledby="new-chat-workspace-label" title={selectedWorkspace?.canonical_path}>
          <FolderIcon class="size-4 text-repository" aria-hidden="true" />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-[13px] font-medium text-heading">{selectedWorkspace ? workspaceTitle(selectedWorkspace) : workspacesLoading ? 'Loading workspaces…' : 'Choose a workspace'}</span>
            {#if selectedWorkspace}
              <span class="block truncate font-mono text-[11px] text-muted-foreground">{selectedWorkspace.display_path}</span>
            {/if}
          </span>
          <span class="font-mono text-[10px] text-muted-foreground" aria-label={`${workspaces.length} workspaces`}>{workspaces.length}</span>
        </Select.Trigger>
        <Select.Content align="start" class="w-[var(--bits-select-anchor-width)]">
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
  </div>
{/if}
