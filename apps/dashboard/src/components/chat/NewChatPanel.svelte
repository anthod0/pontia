<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useDeviceRuntime } from '../../device/context';
  import type { SessionCreationOption, WorkspaceView } from '../../api/types'
  import MessageComposer from './MessageComposer.svelte'
  import SessionTargetSelector from './SessionTargetSelector.svelte'
  import { navigate } from '$lib/navigation'
  import type { ChatCommand } from '$lib/chatCommands'
  const device = useDeviceRuntime();
  const { clearChatDraft } = device.chatDraft;

  interface Props {
    prompt: string
    workspaceId: string
    creationOptionId: string
    creating?: boolean
    canCreate?: boolean
    workspaces: WorkspaceView[]
    workspacesLoading?: boolean
    selectedWorkspace: WorkspaceView | null
    creationOptions: SessionCreationOption[]
    fixedWorkspace?: boolean
    deviceSelector?: Snippet<[boolean]>
    promptDisabled?: boolean
    autofocus?: boolean
    placement?: 'center' | 'bottom'
    onStartChat: (message: string) => Promise<boolean>
  }

  let {
    prompt = $bindable(''),
    workspaceId = $bindable(''),
    creationOptionId = $bindable(''),
    creating = false,
    canCreate = false,
    workspaces,
    workspacesLoading = false,
    selectedWorkspace,
    creationOptions,
    fixedWorkspace = false,
    deviceSelector,
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
    { name: '/compact', description: 'Summarize older context', disabledReason: 'No current session', run: () => {} },
    { name: '/exit', description: 'End the current session', disabledReason: 'No current session', run: () => {} },
  ]
</script>

<div data-testid="new-chat-panel" class:justify-center={placement === 'center'} class:justify-end={placement === 'bottom'} class={`flex min-h-0 shrink-0 flex-col ${placement === 'bottom' ? 'pb-[max(0.5rem,env(safe-area-inset-bottom))] md:pb-[max(1.25rem,env(safe-area-inset-bottom))]' : ''}`}>
  <div class={`mx-auto w-full space-y-3 ${fixedWorkspace ? 'max-w-4xl' : 'max-w-[760px]'}`}>
    <SessionTargetSelector bind:workspaceId bind:creationOptionId {workspaces} {workspacesLoading} {selectedWorkspace} {creationOptions} {fixedWorkspace} {deviceSelector} disabled={creating} />
    <MessageComposer
      bind:value={prompt}
      draftId={`new:${workspaceId}`}
      retainDraft
      {workspaceId}
      {commands}
      inputId="chat-prompt"
      fullscreen
      {autofocus}
      placeholder="Describe a task…"
      disabled={promptDisabled}
      submitDisabled={!canCreate}
      busy={creating}
      submitLabel={creating ? 'Starting session' : 'Start session'}
      startSession
      onSubmit={onStartChat}
    />
  </div>
</div>
