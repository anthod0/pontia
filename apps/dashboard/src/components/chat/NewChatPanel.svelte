<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Snippet } from 'svelte'
  import { useDeviceRuntime } from '../../device/context';
  import type { SessionCreationOption, WorkspaceView } from '../../api/types'
  import ClientModelSelect from './ClientModelSelect.svelte'
  import MessageComposer from './MessageComposer.svelte'
  import SessionTargetSelector from './SessionTargetSelector.svelte'
  import { navigate } from '$lib/navigation'
  import type { ChatCommand } from '$lib/chatCommands'
  const device = useDeviceRuntime();
  const { clearChatDraft } = device.chatDraft;

  interface Props {
    prompt: string
    workspaceId: string
    model: string
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
    model = $bindable(''),
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

  const selectedOption = $derived(creationOptions.find(option => option.id === creationOptionId))

  const commands: ChatCommand[] = $derived([
    {
      name: '/new',
      description: $t('chat.newInWorkspace'),
      run: () => {
        prompt = ''
        clearChatDraft()
        void navigate('/', { workspace: workspaceId })
      },
    },
    { name: '/rename', description: $t('chat.renameCurrent'), disabledReason: $t('chat.noCurrent'), run: () => {} },
    { name: '/compact', description: $t('compaction.description'), disabledReason: $t('chat.noCurrent'), run: () => {} },
    { name: '/exit', description: $t('chat.endCurrent'), disabledReason: $t('chat.noCurrent'), run: () => {} },
  ])
</script>

<div data-testid="new-chat-panel" class:justify-center={placement === 'center'} class:justify-end={placement === 'bottom'} class={`flex min-h-0 shrink-0 flex-col ${placement === 'bottom' ? 'pb-[max(0.5rem,env(safe-area-inset-bottom))] md:pb-[max(1.25rem,env(safe-area-inset-bottom))]' : ''}`}>
  <div class={`mx-auto w-full space-y-3 ${fixedWorkspace ? 'max-w-4xl' : 'max-w-[760px]'}`}>
    <SessionTargetSelector bind:workspaceId bind:creationOptionId {workspaces} {workspacesLoading} {selectedWorkspace} {creationOptions} {fixedWorkspace} {deviceSelector} disabled={creating} />
    {#if selectedOption?.model_catalog_key}
      <ClientModelSelect id="session-model" client={{ name: selectedOption.name, client_type: selectedOption.client_type, client_key: selectedOption.model_catalog_key }} bind:value={model} allowDefault disabled={creating} />
    {/if}
    <MessageComposer
      bind:value={prompt}
      draftId={`new:${workspaceId}`}
      retainDraft
      {workspaceId}
      {commands}
      inputId="chat-prompt"
      fullscreen
      {autofocus}
      placeholder={$t('chat.taskPlaceholder')}
      disabled={promptDisabled}
      submitDisabled={!canCreate}
      busy={creating}
      submitLabel={creating ? $t('chat.starting') : $t('chat.startSession')}
      startSession
      onSubmit={onStartChat}
    />
  </div>
</div>
