<script lang="ts">
  import { onMount } from 'svelte'
  import { navigate } from 'svelte-mini-router'
  import { toast } from 'svelte-sonner'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import { isTransientNetworkError } from '../api/client'
  import { titleFromInitialPrompt } from '$lib/session-chat/sessionChat'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'
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
    sessionsError,
  } from '../stores/sessions'
  import { loadSessionTimeline, resetTimelineState } from '../stores/timeline'

  let createWorkspaceId = ''
  let createClientType = 'pi'
  let creating = false
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let queryWorkspaceSelectionId: string | null = null

  const CLIENT_TYPE_OPTIONS = ['pi']
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = 'pontia.chat.lastWorkspaceId'

  onMount(async () => {
    await Promise.all([loadSessions(), loadWorkspaces()])
    ensureCreateWorkspaceSelection()
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

  function handleNewChatKeydown(event: KeyboardEvent): void {
    const isPlainEnter = event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey
    if (!isPlainEnter) return

    const nextValue = promptValueAfterEnter($chatDraft)
    if (nextValue !== null) {
      event.preventDefault()
      $chatDraft = nextValue
      const textarea = event.currentTarget instanceof HTMLTextAreaElement ? event.currentTarget : null
      queueMicrotask(() => {
        textarea?.setSelectionRange(nextValue.length, nextValue.length)
      })
      return
    }

    event.preventDefault()
    void startChat()
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
      await Promise.all([loadSessionDetail(result.session.session_id), loadSessionTimeline(result.session.session_id, { mode: 'rebuild' })])
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }
</script>

<section class="flex min-h-[calc(100svh-5.5rem)] flex-col md:min-h-[calc(100svh-6.5rem)]">
  <NewChatPanel
    bind:prompt={$chatDraft}
    bind:workspaceId={createWorkspaceId}
    bind:clientType={createClientType}
    {creating}
    {canCreate}
    workspaces={$workspaces}
    workspacesLoading={$workspacesLoading}
    {selectedWorkspace}
    {clientTypeOptions}
    onPromptKeydown={handleNewChatKeydown}
    onStartChat={() => void startChat()}
  />
</section>
