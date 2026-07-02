<script lang="ts">
  import { Folder, Terminal } from '@lucide/svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import FileMentionTextarea from '$lib/components/file-picker/FileMentionTextarea.svelte'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { WorkspaceView } from '../../api/types'
  import { clientTitle, workspaceTitle } from './sessionMetadata'

  interface Props {
    prompt: string
    workspaceId: string
    clientType: string
    creating?: boolean
    canCreate?: boolean
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    clientTypeOptions: string[]
    fixedWorkspace?: boolean
    promptDisabled?: boolean
    onPromptKeydown: (event: KeyboardEvent) => void
    onStartChat: () => void
  }

  let {
    prompt = $bindable(''),
    workspaceId = $bindable(''),
    clientType = $bindable('pi'),
    creating = false,
    canCreate = false,
    workspaces,
    workspacesLoading = false,
    selectedWorkspace,
    clientTypeOptions,
    fixedWorkspace = false,
    promptDisabled = false,
    onPromptKeydown,
    onStartChat,
  }: Props = $props()
</script>

<div data-testid="new-chat-centered-panel" class="flex min-h-0 flex-1 flex-col justify-center">
  <div class="mx-auto w-full max-w-4xl space-y-4">
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

    <div class="space-y-3">
      <PromptInput.Root class="w-full" onSubmit={onStartChat}>
        <PromptInput.Body>
          <FileMentionTextarea id="chat-prompt" bind:value={prompt} workspaceId={workspaceId} placeholder="Ask the agent to implement, inspect, or explain something…" disabled={promptDisabled} shortcutFocusTarget onkeydown={onPromptKeydown} />
        </PromptInput.Body>

        <PromptInput.Toolbar class="justify-end gap-2 pt-0">
          <PromptInput.Submit disabled={!canCreate || creating} aria-label={creating ? 'Starting chat' : 'Start chat'} />
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </div>
  </div>
</div>
