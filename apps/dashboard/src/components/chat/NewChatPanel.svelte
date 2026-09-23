<script lang="ts">
  import type { WorkspaceView } from '../../api/types'
  import MessageComposer from './MessageComposer.svelte'
  import SessionTargetSelector from './SessionTargetSelector.svelte'
  import { navigate } from '$lib/navigation'
  import { clearChatDraft } from '../../stores/chatDraft'
  import type { ChatCommand } from '$lib/chatCommands'

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
    autofocus?: boolean
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
    autofocus = false,
    placement = 'center',
    onStartChat,
  }: Props = $props()

  const commands: ChatCommand[] = [
    {
      name: '/new',
      description: 'Start a new chat in this workspace',
      run: () => {
        prompt = ''
        clearChatDraft()
        void navigate('/', { workspace: workspaceId })
      },
    },
    { name: '/rename', description: 'Rename the current session', disabledReason: 'No current session', run: () => {} },
    { name: '/model', description: 'Choose a model', disabledReason: 'No current session', run: () => {} },
    { name: '/exit', description: 'End the current session', disabledReason: 'No current session', run: () => {} },
  ]
</script>

<div data-testid="new-chat-panel" class:justify-center={placement === 'center'} class:justify-end={placement === 'bottom'} class="flex min-h-0 shrink-0 flex-col">
  <div class={`mx-auto w-full space-y-3 ${fixedWorkspace ? 'max-w-4xl' : 'max-w-[720px]'}`}>
    <SessionTargetSelector bind:workspaceId bind:clientType {workspaces} {workspacesLoading} {selectedWorkspace} {clientTypeOptions} {fixedWorkspace} />
    <MessageComposer
      bind:value={prompt}
      {workspaceId}
      {commands}
      inputId="chat-prompt"
      {autofocus}
      placeholder="What should the agent do?"
      disabled={promptDisabled}
      submitDisabled={!canCreate}
      busy={creating}
      submitLabel={creating ? 'Starting session' : 'Start session'}
      startSession
      onSubmit={onStartChat}
    />
  </div>
</div>
