<script lang="ts">
  import { useDeviceRuntime } from '../device/context';
  import { onDestroy, onMount, type Snippet } from 'svelte'
  import { defaultClientType } from '../clients'
  import { navigate } from '$lib/navigation'
  import { toast } from 'svelte-sonner'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import PontiaMascot from '../components/chat/PontiaMascot.svelte'
  import WorkspaceOnboarding from '../components/workspaces/WorkspaceOnboarding.svelte'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { isTransientNetworkError } from '../api/client'
  import { claimChatEntryAutofocus } from '$lib/chatEntryAutofocus'
  import { titleFromInitialPrompt } from '$lib/session-chat/sessionChat'
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
  let createWorkspaceId = ''
  let creationOptionId = defaultClientType
  let creating = false
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let queryWorkspaceSelectionId: string | null = null
  let autofocusComposer = false
  let initialWorkspaceQueryHandled = false
  let workspaceSetupDecisionPending = true
  let workspaceOnboardingActive = false

  const creationOptionsQuery = createSessionCreationOptionsStore()
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = `pontia.chat.lastWorkspaceId:${device.id}`
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
  $: if (createWorkspaceId && workspaces.length && createWorkspaceId !== queryWorkspaceSelectionId && createWorkspaceId !== availableWorkspaceId(readQueryWorkspaceId())) rememberCreateWorkspaceSelection(createWorkspaceId)
  $: canCreate = Boolean(createWorkspaceId && selectedCreationOption && !$creationOptionsQuery.isError && !creating)
  $: passiveErrorMessage = $creationOptionsQuery.error?.message ?? (workspacesError && !isTransientNetworkError(workspacesError) ? workspacesError : null)
  $: errorMessage = actionError ?? passiveErrorMessage
  $: {
    if (errorMessage && errorMessage !== lastToastedError) {
      toast.error(actionError ? 'Could not start session' : $creationOptionsQuery.isError ? 'Could not load agents' : 'Could not load workspaces', {
        description: errorMessage,
        ...(!actionError ? {
          action: {
            label: 'Retry',
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
    try {
      return window.localStorage.getItem(LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY)
    } catch {
      return null
    }
  }

  function rememberCreateWorkspaceSelection(workspaceId: string): void {
    if (!workspaceId || !workspaces.some((workspace) => workspace.workspace_id === workspaceId)) return
    try {
      window.localStorage.setItem(LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY, workspaceId)
    } catch {
      // Ignore unavailable storage; the workspace selector should still work.
    }
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
        workspace_id: workspaceId,
        title: titleFromInitialPrompt(initialPrompt),
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

{#if !workspaces.length && (!initialWorkspaceLoadComplete || workspaceSetupDecisionPending)}
  <section class="flex min-h-[calc(100svh-5.5rem)] items-center justify-center md:min-h-[calc(100svh-6.5rem)]" aria-label="Loading dashboard">
    <div class="w-full max-w-3xl space-y-3">
      <Skeleton class="h-24 w-full" />
      <Skeleton class="h-64 w-full" />
    </div>
  </section>
{:else if workspaceOnboardingActive}
  <WorkspaceOnboarding canContinue={workspaces.length > 0} onContinue={completeWorkspaceOnboarding} />
{:else}
  <section class="flex min-h-[calc(100svh-3.25rem)] flex-col gap-6">
    <div class="mx-auto flex w-full max-w-[720px] flex-1 items-center justify-center py-12 text-center">
      <div>
        <PontiaMascot class="mx-auto mb-4 size-20" />
        <h1 class="mb-2 text-[28px] font-semibold tracking-tight text-heading">Start a session</h1>
        <p class="text-sm text-foreground">Choose a workspace and agent client, then describe the job.</p>
      </div>
    </div>
    <NewChatPanel
      bind:prompt={$chatDraft}
      bind:workspaceId={createWorkspaceId}
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
  </section>
{/if}
