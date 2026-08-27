<script lang="ts">
  import { onMount } from 'svelte'
  import { navigate } from '$lib/navigation'
  import { toast } from 'svelte-sonner'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import AgentOverview from '../components/home/AgentOverview.svelte'
  import WorkspaceOnboarding from '../components/workspaces/WorkspaceOnboarding.svelte'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { isTransientNetworkError } from '../api/client'
  import { claimChatEntryAutofocus } from '$lib/chatEntryAutofocus'
  import { titleFromInitialPrompt } from '$lib/session-chat/sessionChat'
  import { chatDraft, clearChatDraft } from '../stores/chatDraft'
  import { rememberOptimisticInitialMessage } from '../stores/optimisticChat'
  import {
    loadWorkspaces,
    workspaces,
    workspacesError,
    workspacesLoading,
  } from '../stores/workspaces'
  import {
    createSession,
    loadSessionDetail,
    loadSessions,
    sessions,
    sessionsError,
    sessionsLoading,
  } from '../stores/sessions'
  import { loadSessionTimeline, resetTimelineState } from '../stores/timeline'

  let createWorkspaceId = ''
  let createClientType = 'pi'
  let creating = false
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let queryWorkspaceSelectionId: string | null = null
  let overviewWorkspaceId: string | null = null
  let autofocusComposer = false
  let initialWorkspaceLoadComplete = false

  const CLIENT_TYPE_OPTIONS = ['pi', 'claude']
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = 'pontia.chat.lastWorkspaceId'

  onMount(() => {
    let mounted = true
    const handleLocationChange = () => syncWorkspaceSelectionsFromLocation()
    window.addEventListener('popstate', handleLocationChange)
    if ($workspaces.length) autofocusComposer = claimChatEntryAutofocus('/')
    void Promise.all([loadSessions(), loadWorkspaces()])
      .then(syncWorkspaceSelectionsFromLocation)
      .catch(() => {
        // The stores expose request failures; keep the page renderable without an unhandled rejection.
      })
      .finally(() => {
        if (mounted) initialWorkspaceLoadComplete = true
      })
    return () => {
      mounted = false
      window.removeEventListener('popstate', handleLocationChange)
    }
  })

  $: if ($workspaces.length) ensureCreateWorkspaceSelection()
  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: clientTypeOptions = CLIENT_TYPE_OPTIONS
  $: if (!clientTypeOptions.includes(createClientType)) createClientType = clientTypeOptions[0] ?? createClientType
  $: if (createWorkspaceId && $workspaces.length && createWorkspaceId !== queryWorkspaceSelectionId && createWorkspaceId !== availableWorkspaceId(readQueryWorkspaceId())) rememberCreateWorkspaceSelection(createWorkspaceId)
  $: canCreate = Boolean($chatDraft.trim() && createWorkspaceId && createClientType.trim() && !creating)
  $: rawPassiveErrorMessage = $sessionsError ?? $workspacesError
  $: passiveErrorMessage = rawPassiveErrorMessage && !isTransientNetworkError(rawPassiveErrorMessage) ? rawPassiveErrorMessage : null
  $: errorMessage = actionError ?? passiveErrorMessage
  $: {
    if (errorMessage && errorMessage !== lastToastedError) {
      toast.error('Chat error', { description: errorMessage })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }

  function readQueryWorkspaceId(): string | null {
    return new URLSearchParams(window.location.search).get('workspace')
  }

  function availableWorkspaceId(workspaceId: string | null): string | null {
    if (!workspaceId) return null
    return $workspaces.some((workspace) => workspace.workspace_id === workspaceId) ? workspaceId : null
  }

  function readRememberedWorkspaceId(): string | null {
    try {
      return window.localStorage.getItem(LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY)
    } catch {
      return null
    }
  }

  function rememberCreateWorkspaceSelection(workspaceId: string): void {
    if (!workspaceId || !$workspaces.some((workspace) => workspace.workspace_id === workspaceId)) return
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
    return $workspaces[0]?.workspace_id ?? ''
  }

  function syncWorkspaceSelectionsFromLocation(): void {
    overviewWorkspaceId = availableWorkspaceId(readQueryWorkspaceId())
    ensureCreateWorkspaceSelection()
  }

  function selectOverviewWorkspace(workspaceId: string | null): void {
    overviewWorkspaceId = workspaceId
    if (workspaceId) {
      createWorkspaceId = workspaceId
      rememberCreateWorkspaceSelection(workspaceId)
    }
    void navigate('/', { workspace: workspaceId })
  }

  function ensureCreateWorkspaceSelection(): void {
    if (!$workspaces.length) return
    const queryWorkspaceId = availableWorkspaceId(readQueryWorkspaceId())
    if (queryWorkspaceId) {
      queryWorkspaceSelectionId = queryWorkspaceId
      if (createWorkspaceId !== queryWorkspaceId) createWorkspaceId = queryWorkspaceId
      return
    }
    queryWorkspaceSelectionId = null
    if (createWorkspaceId && $workspaces.some((workspace) => workspace.workspace_id === createWorkspaceId)) return
    createWorkspaceId = preferredCreateWorkspaceId()
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      const initialPrompt = $chatDraft.trim()
      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        title: titleFromInitialPrompt(initialPrompt),
        initial_task: { input: initialPrompt, metadata: { source: 'dashboard_chat' } },
        metadata: { source: 'dashboard_chat' },
      })
      rememberCreateWorkspaceSelection(createWorkspaceId)
      rememberOptimisticInitialMessage(result.session.session_id, initialPrompt, result.initial_turn)
      clearChatDraft()
      resetTimelineState(result.session.session_id)
      navigate(`/chat/${result.session.session_id}`)
      await Promise.all([
        loadSessionDetail(result.session.session_id),
        loadSessionTimeline(result.session.session_id, {
          mode: 'rebuild',
          ...(result.session.capabilities.topology === true ? { topology: true } : {}),
        }),
      ])
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }
</script>

{#if !$workspaces.length && !initialWorkspaceLoadComplete}
  <section class="flex min-h-[calc(100svh-5.5rem)] items-center justify-center md:min-h-[calc(100svh-6.5rem)]" aria-label="Loading dashboard">
    <div class="w-full max-w-3xl space-y-3">
      <Skeleton class="h-24 w-full" />
      <Skeleton class="h-64 w-full" />
    </div>
  </section>
{:else if !$workspaces.length && !$workspacesError}
  <WorkspaceOnboarding />
{:else}
  <section class="flex min-h-[calc(100svh-5.5rem)] flex-col gap-8 md:min-h-[calc(100svh-6.5rem)]">
    <AgentOverview
      sessions={$sessions}
      workspaces={$workspaces}
      loading={$sessionsLoading}
      selectedWorkspaceId={overviewWorkspaceId}
      onWorkspaceChange={selectOverviewWorkspace}
    />

    <NewChatPanel
      bind:prompt={$chatDraft}
      bind:workspaceId={createWorkspaceId}
      bind:clientType={createClientType}
      {creating}
      {canCreate}
      autofocus={autofocusComposer}
      workspaces={$workspaces}
      workspacesLoading={$workspacesLoading}
      {selectedWorkspace}
      {clientTypeOptions}
      placement="bottom"
      onStartChat={() => void startChat()}
    />
  </section>
{/if}
