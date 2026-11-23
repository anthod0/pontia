<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import { onDestroy, onMount, type Snippet } from 'svelte'
  import { defaultClientType } from '../clients'
  import { navigate } from '$lib/navigation'
  import { toast } from 'svelte-sonner'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import NewChatLayout from '../components/chat/NewChatLayout.svelte'
  import NewChatComposerPlaceholder from '../components/chat/NewChatComposerPlaceholder.svelte'
  import WorkspaceOnboarding from '../components/workspaces/WorkspaceOnboarding.svelte'
  import { isTransientNetworkError } from '../api/client'
  import { claimChatEntryAutofocus } from '$lib/chatEntryAutofocus'
  import { titleFromInitialPrompt } from '$lib/session-chat/sessionChat'
  import { readStored, scopedKey, writeStored } from '$lib/storage'
  const device = useDeviceRuntime();
  const { createSessionCreationOptionsStore } = device.sessionCreation;
  const { chatDraft, clearChatDraft } = device.chatDraft;
  const { createWorkspacesStore } = device.workspaceQueries;
  const { createSession, loadSessionDetail } = device.sessions;
  const { loadSessionTimeline, resetTimelineState } = device.timeline;

  let mounted = true
  onDestroy(() => { mounted = false })
  export let deviceSelector: Snippet<[boolean]> | undefined = undefined
  export let routeScope: string | undefined = undefined
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = scopedKey('pontia.chat.lastWorkspaceId', device.id)
  const LAST_NEW_CHAT_CLIENT_STORAGE_KEY = scopedKey('pontia.chat.lastCreationOptionId', device.id)
  let createWorkspaceId = ''
  let creationOptionId = readStored(LAST_NEW_CHAT_CLIENT_STORAGE_KEY) ?? defaultClientType
  let reasoningEffort = ''
  let model = ''
  let modelOptionId = creationOptionId
  let modelWorkspaceId = createWorkspaceId
  $: if (creationOptionId !== modelOptionId || createWorkspaceId !== modelWorkspaceId) {
    modelOptionId = creationOptionId
    modelWorkspaceId = createWorkspaceId
    reasoningEffort = ''
    model = ''
  }
  let creating = false
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let queryWorkspaceSelectionId: string | null = null
  let autofocusComposer = false
  let initialWorkspaceQueryHandled = false
  let workspaceSetupDecisionPending = true
  let workspaceOnboardingActive = false

  const creationOptionsQuery = createSessionCreationOptionsStore()
  const workspacesQuery = createWorkspacesStore()

  onMount(() => {
    const handleLocationChange = () => syncWorkspaceSelectionsFromLocation()
    window.addEventListener('popstate', handleLocationChange)
    syncWorkspaceSelectionsFromLocation()
    return () => {
      window.removeEventListener('popstate', handleLocationChange)
    }
  })

  $: workspaces = $workspacesQuery.data ?? []
  $: workspacesError = $workspacesQuery.error?.message ?? null
  $: workspacesLoading = $workspacesQuery.isFetching
  $: initialWorkspaceLoadComplete = !$workspacesQuery.isPending
  $: workspaceSetupLoading = !workspaces.length && (!initialWorkspaceLoadComplete || workspaceSetupDecisionPending)
  $: if (initialWorkspaceLoadComplete && workspaceSetupDecisionPending && !workspacesLoading) {
    workspaceOnboardingActive = !workspaces.length && !workspacesError
    workspaceSetupDecisionPending = false
  }

  $: if (workspaces.length) ensureCreateWorkspaceSelection()
  $: if (!$workspacesQuery.isPending && !initialWorkspaceQueryHandled) {
    if (workspaces.length) autofocusComposer = claimChatEntryAutofocus(`${device.id}:/`)
    initialWorkspaceQueryHandled = true
  }
  $: selectedWorkspace = workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: creationOptions = $creationOptionsQuery.data ?? []
  $: if (creationOptions.length && !creationOptions.some((option) => option.id === creationOptionId)) creationOptionId = creationOptions[0].id
  $: selectedCreationOption = creationOptions.find((option) => option.id === creationOptionId)
  $: if (selectedCreationOption) writeStored(LAST_NEW_CHAT_CLIENT_STORAGE_KEY, selectedCreationOption.id)
  $: if (createWorkspaceId && workspaces.length && createWorkspaceId !== queryWorkspaceSelectionId && createWorkspaceId !== availableWorkspaceId(readQueryWorkspaceId())) rememberCreateWorkspaceSelection(createWorkspaceId)
  $: canCreate = Boolean(createWorkspaceId && selectedCreationOption && !$creationOptionsQuery.isError && !creating)
  $: passiveErrorMessage = $creationOptionsQuery.error?.message ?? (workspacesError && !isTransientNetworkError(workspacesError) ? workspacesError : null)
  $: errorMessage = actionError ?? passiveErrorMessage
  $: {
    if (errorMessage && errorMessage !== lastToastedError) {
      toast.error(actionError ? $t('chat.startFailed') : $creationOptionsQuery.isError ? $t('clients.loadFailed') : $t('workspaces.loadFailed'), {
        description: errorMessage,
        ...(!actionError ? {
          action: {
            label: $t('common.retry'),
            onClick: () => {
              lastToastedError = null
              if ($creationOptionsQuery.isError) void $creationOptionsQuery.refetch()
              else void retryWorkspaces()
            },
          },
        } : {}),
      })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }

  function readQueryWorkspaceId(): string | null {
    return new URLSearchParams(window.location.search).get('workspace')
  }

  function availableWorkspaceId(workspaceId: string | null): string | null {
    if (!workspaceId) return null
    return workspaces.some((workspace) => workspace.workspace_id === workspaceId) ? workspaceId : null
  }

  function readRememberedWorkspaceId(): string | null {
    return readStored(LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY)
  }

  function rememberCreateWorkspaceSelection(workspaceId: string): void {
    if (!workspaceId || !workspaces.some((workspace) => workspace.workspace_id === workspaceId)) return
    writeStored(LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY, workspaceId)
  }

  function preferredCreateWorkspaceId(): string {
    const queryWorkspaceId = availableWorkspaceId(readQueryWorkspaceId())
    if (queryWorkspaceId) return queryWorkspaceId
    const rememberedWorkspaceId = availableWorkspaceId(readRememberedWorkspaceId())
    if (rememberedWorkspaceId) return rememberedWorkspaceId
    return workspaces[0]?.workspace_id ?? ''
  }

  function syncWorkspaceSelectionsFromLocation(): void {
    ensureCreateWorkspaceSelection()
  }

  async function retryWorkspaces(): Promise<void> {
    workspaceSetupDecisionPending = true
    await $workspacesQuery.refetch()
  }

  function completeWorkspaceOnboarding(): void {
    if (!workspaces.length) return
    ensureCreateWorkspaceSelection()
    workspaceOnboardingActive = false
  }

  function ensureCreateWorkspaceSelection(): void {
    if (!workspaces.length) return
    const queryWorkspaceId = availableWorkspaceId(readQueryWorkspaceId())
    if (queryWorkspaceId) {
      queryWorkspaceSelectionId = queryWorkspaceId
      if (createWorkspaceId !== queryWorkspaceId) createWorkspaceId = queryWorkspaceId
      return
    }
    queryWorkspaceSelectionId = null
    if (createWorkspaceId && workspaces.some((workspace) => workspace.workspace_id === createWorkspaceId)) return
    createWorkspaceId = preferredCreateWorkspaceId()
  }

  async function startChat(initialPrompt: string): Promise<boolean> {
    if (!canCreate || !selectedCreationOption || !initialPrompt.trim()) return false
    const workspaceId = createWorkspaceId
    creating = true
    actionError = null
    try {
      const result = await createSession({
        client_type: selectedCreationOption.client_type,
        ...(selectedCreationOption.model_catalog_key && model ? { model } : {}),
        ...(reasoningEffort ? { reasoning_effort: reasoningEffort } : {}),
        workspace_id: workspaceId,
        title: titleFromInitialPrompt(initialPrompt),
        include_standalone_session_instructions: true,
        initial_task: { input: initialPrompt, metadata: { source: 'dashboard_chat' } },
        metadata: { ...selectedCreationOption.metadata, source: 'dashboard_chat', title_source: 'input' },
      })
      if (!mounted) return true
      rememberCreateWorkspaceSelection(workspaceId)
      clearChatDraft()
      resetTimelineState(result.session.session_id)
      if (routeScope === undefined) void navigate(`/chat/${result.session.session_id}`)
      else void navigate(`/chat/${result.session.session_id}`, undefined, { scope: routeScope })
      void Promise.all([
        loadSessionDetail(result.session.session_id),
        loadSessionTimeline(result.session.session_id, {
          mode: 'rebuild',
          ...(result.session.capabilities.topology === true ? { topology: true } : {}),
        }),
      ]).catch(error => { actionError = error instanceof Error ? error.message : String(error) })
      return true
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
      return false
    } finally {
      creating = false
    }
  }
</script>

{#if workspaceOnboardingActive && !workspaceSetupLoading}
  <WorkspaceOnboarding canContinue={workspaces.length > 0} onContinue={completeWorkspaceOnboarding} {routeScope} />
{:else}
  <NewChatLayout status={workspaceSetupLoading ? { mood: 'loading', title: $t('workspaces.loading'), description: $t('common.fewSeconds') } : undefined}>
    {#if workspaceSetupLoading}
      <NewChatComposerPlaceholder {deviceSelector} />
    {:else}
      <NewChatPanel
        bind:prompt={$chatDraft}
        bind:workspaceId={createWorkspaceId}
        bind:reasoningEffort
        bind:model
        bind:creationOptionId
        {creating}
        {deviceSelector}
        {canCreate}
        autofocus={autofocusComposer}
        {workspaces}
        {workspacesLoading}
        {selectedWorkspace}
        {creationOptions}
        placement="bottom"
        onStartChat={startChat}
      />
    {/if}
  </NewChatLayout>
{/if}
