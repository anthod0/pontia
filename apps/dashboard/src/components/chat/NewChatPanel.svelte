<script lang="ts">
  import type { WorkspaceView } from '../../api/types'
  import MessageComposer from './MessageComposer.svelte'
  import SessionTargetSelector from './SessionTargetSelector.svelte'

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
    placement?: 'center' | 'bottom'
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
    placement = 'center',
    onStartChat,
  }: Props = $props()
</script>

<div data-testid="new-chat-panel" class:justify-center={placement === 'center'} class:justify-end={placement === 'bottom'} class="flex min-h-0 flex-1 flex-col">
  <div class="mx-auto w-full max-w-4xl space-y-4">
    <SessionTargetSelector bind:workspaceId bind:clientType {workspaces} {workspacesLoading} {selectedWorkspace} {clientTypeOptions} {fixedWorkspace} />
    <MessageComposer
      bind:value={prompt}
      {workspaceId}
      inputId="chat-prompt"
      placeholder="Ask the agent to implement, inspect, or explain something…"
      disabled={promptDisabled}
      submitDisabled={!canCreate}
      busy={creating}
      submitLabel={creating ? 'Starting chat' : 'Start chat'}
      onSubmit={onStartChat}
    />
  </div>
</div>
