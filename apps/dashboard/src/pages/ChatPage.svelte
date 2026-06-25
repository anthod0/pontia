<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { toast } from 'svelte-sonner'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import { isTransientNetworkError } from '../api/client'
  import type { DashboardStreamEvent, InboxMessageView, SessionView } from '../api/types'
  import {
    canSendSessionMessage,
    timelineItemsToChatMessages,
    titleFromInitialPrompt,
    turnsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import {
    chatMessagesWithOptimistic,
    optimisticInitialMessages,
    rememberOptimisticInitialMessage,
  } from '../stores/optimisticChat'
  import {
    loadWorkspaces,
    refreshWorkspaceGitStatus,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    workspaces,
    workspacesError,
    workspacesLoading,
  } from '../stores/workspaces'
  import {
    cancelInboxMessage,
    createSession,
    dismissInboxMessage,
    loadSessionDetail,
    loadSessions,
    interruptSession,
    restartSession,
    resumeSession,
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    sessions,
    sessionsError,
    submitInboxMessage,
    terminateSession,
    updateSessionTitle,
  } from '../stores/sessions'
  import {
    handleTimelineMessageUpdated,
    loadSessionTimeline,
    resetTimelineState,
    timelineState,
  } from '../stores/timeline'
  import { subscribeDashboardEvents } from '../services/eventStream'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import SessionComposerDock from '../components/chat/SessionComposerDock.svelte'
  import InboxSheet from '../components/chat/InboxSheet.svelte'
  import RenameSessionDialog from '../components/chat/RenameSessionDialog.svelte'
  import { sessionMetadataItems, sessionMetadataSummary, visibleChatInboxMessages } from '../components/chat/sessionMetadata'

  let selectedSessionId = ''
  let prompt = ''
  let createWorkspaceId = ''
  let createClientType = 'pi'
  let creating = false

  let input = ''
  let submitting = false
  let actionBusy = false
  let inboxActionMessageId: string | null = null
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let inboxSheetOpen = false
  let renameSessionDialogOpen = false
  let queryWorkspaceSelectionId: string | null = null
  let unsubscribeDashboardEvents: (() => void) | null = null
  let foregroundRefreshInFlight: Promise<void> | null = null


  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const CLIENT_TYPE_OPTIONS = ['pi']
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = 'pontia.chat.lastWorkspaceId'
  const newChatSelectorTriggerClass = 'h-7 rounded-full px-3 text-sm font-normal text-muted-foreground'

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces()])
    ensureCreateWorkspaceSelection()
    if (selectedSessionId) {
      await loadSelectedSession(selectedSessionId)
      await refreshSessionGitStatus(currentSelectedSession())
    }
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
    window.addEventListener('focus', handleForegroundResume)
    window.addEventListener('pageshow', handleForegroundResume)
    document.addEventListener('visibilitychange', handleVisibilityChange)
  })

  onDestroy(() => {
    unsubscribeDashboardEvents?.()
    window.removeEventListener('focus', handleForegroundResume)
    window.removeEventListener('pageshow', handleForegroundResume)
    document.removeEventListener('visibilitychange', handleVisibilityChange)
  })

  $: selectedSession = selectedSessionId ? ($sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.session : $sessions.find((session) => session.session_id === selectedSessionId) ?? null) : null
  $: selectedSessionGitStatus = selectedSession ? $workspaceGitStatuses[selectedSession.workspace_id ?? ''] : undefined
  $: selectedSessionMetadataItems = selectedSession ? sessionMetadataItems(selectedSession, $workspaces, selectedSessionGitStatus, $workspaceGitStatusErrors) : []
  $: selectedSessionMetadataSummary = sessionMetadataSummary(selectedSessionMetadataItems)
  $: timelineMessages = $timelineState.sessionId === selectedSessionId ? timelineItemsToChatMessages($timelineState.items) : []
  $: projectedTurnMessages = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? turnsToChatMessages($sessionDetail.turns) : []
  $: messages = chatMessagesWithOptimistic(selectedSessionId, timelineMessages.length ? timelineMessages : projectedTurnMessages, $optimisticInitialMessages)
  $: selectedInboxMessages = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.inboxMessages : []
  $: visibleInboxMessages = visibleChatInboxMessages(selectedInboxMessages)
  $: inboxActionableCount = visibleInboxMessages.filter((message) => message.state === 'pending' || message.state === 'failed').length
  $: if ($workspaces.length && !selectedSessionId) ensureCreateWorkspaceSelection()
  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: clientTypeOptions = CLIENT_TYPE_OPTIONS
  $: if (!clientTypeOptions.includes(createClientType)) createClientType = clientTypeOptions[0] ?? createClientType
  $: if (createWorkspaceId && $workspaces.length && createWorkspaceId !== queryWorkspaceSelectionId && createWorkspaceId !== availableWorkspaceId(readQueryWorkspaceId())) rememberCreateWorkspaceSelection(createWorkspaceId)
  $: canCreate = Boolean(prompt.trim() && createWorkspaceId && createClientType.trim() && !creating)
  $: canSend = canSendSessionMessage(selectedSession, input) && !submitting
  $: rawPassiveErrorMessage = $sessionDetailError ?? $timelineState.error ?? $sessionsError ?? $workspacesError
  $: passiveErrorMessage = rawPassiveErrorMessage && !isTransientNetworkError(rawPassiveErrorMessage) ? rawPassiveErrorMessage : null
  $: errorMessage = actionError ?? passiveErrorMessage
  $: shouldToastError = Boolean(errorMessage)
  $: {
    if (errorMessage && shouldToastError && errorMessage !== lastToastedError) {
      toast.error('Chat error', { description: errorMessage })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }


  function requestedSessionIdFromLocation(): string {
    return getPathParams().sessionId ?? new URLSearchParams(window.location.search).get('session') ?? ''
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

  function currentSelectedSession(): SessionView | null {
    if (!selectedSessionId) return null
    const detail = get(sessionDetail)
    if (detail?.session.session_id === selectedSessionId) return detail.session
    return get(sessions).find((session) => session.session_id === selectedSessionId) ?? null
  }

  const gitStatusRefreshesInFlight = new Map<string, Promise<void>>()

  async function refreshSessionGitStatus(session: SessionView | null): Promise<void> {
    const workspaceId = session?.workspace_id
    if (!workspaceId) return
    const existing = gitStatusRefreshesInFlight.get(workspaceId)
    if (existing) {
      await existing
      return
    }
    const refresh = refreshWorkspaceGitStatus(workspaceId).finally(() => {
      if (gitStatusRefreshesInFlight.get(workspaceId) === refresh) gitStatusRefreshesInFlight.delete(workspaceId)
    })
    gitStatusRefreshesInFlight.set(workspaceId, refresh)
    await refresh
  }

  async function refreshCurrentSessionGitStatus(): Promise<void> {
    await refreshSessionGitStatus(currentSelectedSession())
  }

  async function cancelPendingInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || message.state !== 'pending') return
    inboxActionMessageId = message.message_id
    actionError = null
    try {
      await cancelInboxMessage(selectedSessionId, message.message_id)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      inboxActionMessageId = null
    }
  }

  async function retryFailedInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || message.state !== 'failed') return
    inboxActionMessageId = message.message_id
    actionError = null
    try {
      await submitInboxMessage(selectedSessionId, {
        input: message.input.summary,
        delivery_policy: message.delivery_policy === 'interrupt_now' ? 'interrupt_now' : 'after_idle',
        metadata: message.metadata,
      })
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      inboxActionMessageId = null
    }
  }

  async function dismissFailedInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || message.state !== 'failed') return
    inboxActionMessageId = message.message_id
    actionError = null
    try {
      await dismissInboxMessage(selectedSessionId, message.message_id)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      inboxActionMessageId = null
    }
  }

  function handleForegroundResume(): void {
    if (document.visibilityState === 'hidden') return
    const sessionId = selectedSessionId
    if (!sessionId) return
    if (foregroundRefreshInFlight) return

    const currentTimeline = get(timelineState)
    const timelineRefresh = currentTimeline.sessionId === sessionId && currentTimeline.items.length
      ? handleTimelineMessageUpdated(sessionId)
      : loadSessionTimeline(sessionId, { mode: 'rebuild' })

    foregroundRefreshInFlight = Promise.all([
      loadSessionDetail(sessionId, { showLoading: false }),
      timelineRefresh,
    ]).then(() => undefined).finally(() => {
      foregroundRefreshInFlight = null
    })
  }

  function handleVisibilityChange(): void {
    handleForegroundResume()
    if (document.visibilityState === 'hidden') return
    void refreshCurrentSessionGitStatus()
  }

  function isSessionIdleEvent(eventType: string): boolean {
    return eventType === 'session.ready'
      || eventType === 'turn.completed'
      || eventType === 'turn.failed'
      || eventType === 'turn.interrupted'
      || eventType === 'turn.cancelled'
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === 'session_event') {
      if (streamEvent.event.session_id !== selectedSessionId) return
      if (isSessionIdleEvent(streamEvent.event.type)) {
        void refreshCurrentSessionGitStatus()
        return
      }
      if (streamEvent.event.type !== 'session.message_updated') return
      const rawBindingId = streamEvent.event.payload.binding_id
      const bindingId = typeof rawBindingId === 'string' ? rawBindingId : null
      void handleTimelineMessageUpdated(selectedSessionId, bindingId)
      return
    }
  }

  function openRenameSelectedSessionDialog(): void {
    if (!selectedSessionId || !selectedSession) return
    actionError = null
    renameSessionDialogOpen = true
  }

  async function renameSelectedSession(title: string | null): Promise<void> {
    if (!selectedSessionId || !selectedSession) return
    actionBusy = true
    actionError = null
    try {
      await updateSessionTitle(selectedSessionId, title)
      renameSessionDialogOpen = false
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  function openSessionConsole(): void {
    navigate(selectedSessionId ? `/sessions/${selectedSessionId}` : '/sessions')
  }

  function openNewChat(workspaceId?: string | null): void {
    const queryWorkspaceId = workspaceId?.trim() || null
    selectedSessionId = ''
    actionError = null
    resetTimelineState()
    if (queryWorkspaceId) {
      const availableQueryWorkspaceId = availableWorkspaceId(queryWorkspaceId)
      queryWorkspaceSelectionId = availableQueryWorkspaceId
      if (availableQueryWorkspaceId) createWorkspaceId = availableQueryWorkspaceId
      navigate('/chat', { workspace: queryWorkspaceId })
      return
    }
    queryWorkspaceSelectionId = null
    navigate('/chat')
  }

  async function selectSessionFromLocation(): Promise<void> {
    const nextSessionId = requestedSessionIdFromLocation()
    if (nextSessionId === selectedSessionId) return
    selectedSessionId = nextSessionId
    input = ''
    actionError = null
    if (selectedSessionId) {
      await loadSelectedSession(selectedSessionId)
      await refreshSessionGitStatus(currentSelectedSession())
    } else {
      resetTimelineState()
      ensureCreateWorkspaceSelection()
    }
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    const currentTimeline = get(timelineState)
    const hasLoadedTimeline = currentTimeline.sessionId === sessionId && currentTimeline.items.length > 0
    if (!hasLoadedTimeline) resetTimelineState(sessionId)
    await Promise.all([
      loadSessionDetail(sessionId),
      hasLoadedTimeline ? handleTimelineMessageUpdated(sessionId) : loadSessionTimeline(sessionId, { mode: 'rebuild' }),
    ])
  }

  function handleNewChatKeydown(event: KeyboardEvent): void {
    const shouldSubmit = event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey
    if (shouldSubmit) {
      event.preventDefault()
      void startChat()
    }
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      const initialPrompt = prompt.trim()
      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        title: titleFromInitialPrompt(initialPrompt),
        initial_task: { input: initialPrompt, metadata: { source: 'dashboard_chat' } },
        metadata: { source: 'dashboard_chat' },
      })
      selectedSessionId = result.session.session_id
      rememberOptimisticInitialMessage(result.session.session_id, initialPrompt, result.initial_turn)
      prompt = ''
      resetTimelineState(result.session.session_id)
      navigate(`/chat/${result.session.session_id}`)
      await Promise.all([loadSessionDetail(result.session.session_id), loadSessionTimeline(result.session.session_id, { mode: 'rebuild' })])
      await refreshSessionGitStatus(currentSelectedSession())
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }

  async function loadEarlierMessages(): Promise<void> {
    if (!selectedSessionId || !$timelineState.hasMore || $timelineState.refreshing) return
    actionError = null
    try {
      await loadSessionTimeline(selectedSessionId, { mode: 'more' })
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    }
  }

  async function interruptSelectedSession(): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    try {
      await interruptSession(selectedSessionId)
      await loadSessionTimeline(selectedSessionId, { mode: 'rebuild' })
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  async function runSessionLifecycle(action: 'exit' | 'resume' | 'restart'): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    try {
      if (action === 'exit') await terminateSession(selectedSessionId)
      if (action === 'resume') await resumeSession(selectedSessionId)
      if (action === 'restart') await restartSession(selectedSessionId)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  function sessionStateFromStores(sessionId: string): string | null {
    const detail = get(sessionDetail)
    if (detail?.session.session_id === sessionId) return detail.session.state
    return get(sessions).find((session) => session.session_id === sessionId)?.state ?? null
  }

  function waitForSessionIdle(sessionId: string, timeoutMs = AUTO_RESUME_IDLE_TIMEOUT_MS): Promise<void> {
    if (sessionStateFromStores(sessionId) === 'idle') return Promise.resolve()

    return new Promise((resolve, reject) => {
      let done = false
      let unsubscribeSessions: (() => void) | null = null
      let unsubscribeDetail: (() => void) | null = null

      const cleanup = () => {
        unsubscribeSessions?.()
        unsubscribeDetail?.()
        clearTimeout(timeout)
      }
      const finish = (callback: () => void) => {
        if (done) return
        done = true
        cleanup()
        callback()
      }
      const check = () => {
        if (sessionStateFromStores(sessionId) === 'idle') finish(resolve)
      }
      const timeout = setTimeout(() => {
        finish(() => reject(new Error('Session resume timed out before becoming idle.')))
      }, timeoutMs)

      unsubscribeSessions = sessions.subscribe(check)
      unsubscribeDetail = sessionDetail.subscribe(check)
      check()
    })
  }

  async function sendMessage(): Promise<void> {
    if (!canSend || !selectedSessionId) return
    submitting = true
    actionError = null
    const message = input.trim()
    try {
      if (selectedSession?.state === 'exited') {
        await resumeSession(selectedSessionId)
        await waitForSessionIdle(selectedSessionId)
      }
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: 'after_idle',
        metadata: { source: 'dashboard_chat' },
      })
      input = ''
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submitting = false
    }
  }
</script><svelte:window onpopstate={() => void selectSessionFromLocation()} />

<section class={selectedSessionId ? 'flex flex-col gap-4 pb-40' : 'flex min-h-[calc(100svh-5.5rem)] flex-col md:min-h-[calc(100svh-6.5rem)]'}>
  {#if !selectedSessionId}
    <NewChatPanel
      bind:prompt
      bind:workspaceId={createWorkspaceId}
      bind:clientType={createClientType}
      {creating}
      {canCreate}
      workspaces={$workspaces}
      workspacesLoading={$workspacesLoading}
      {selectedWorkspace}
      {clientTypeOptions}
      selectorTriggerClass={newChatSelectorTriggerClass}
      onPromptKeydown={handleNewChatKeydown}
      onStartChat={() => void startChat()}
    />
  {:else}
    <div class="mx-auto min-w-0 w-full max-w-4xl flex-1">
      <div class="flex min-w-0 flex-col rounded-xl bg-transparent">
        {#if $sessionDetailLoading && !selectedSession}
          <div class="space-y-4 p-6"><Skeleton class="h-10 w-1/3" /><Skeleton class="h-80 w-full" /></div>
        {:else if !selectedSession}
          <Empty.Root class="h-full">
            <Empty.Header>
              <Empty.Title>Session not found</Empty.Title>
              <Empty.Description>Start a new chat or select a recent session from the sidebar.</Empty.Description>
            </Empty.Header>
            <Empty.Content><Button onclick={() => openNewChat()}>Start a new chat</Button></Empty.Content>
          </Empty.Root>
        {:else}
          <SessionConversation
            {messages}
            sessionState={selectedSession.state}
            loading={($sessionDetailLoading || $timelineState.loading) && !messages.length}
            interruptEnabled={selectedSession.state === 'busy' && selectedSession.capabilities.interrupt === true}
            interruptBusy={actionBusy}
            hasMoreHistory={$timelineState.hasMore}
            historyLoading={$timelineState.refreshKind === 'history'}
            autoScrollKey={$timelineState.sessionId === selectedSessionId ? $timelineState.tailCursor : null}
            onInterrupt={() => void interruptSelectedSession()}
            onLoadMoreHistory={loadEarlierMessages}
          />

          <SessionComposerDock
            bind:input
            session={selectedSession}
            gitStatus={selectedSessionGitStatus}
            gitStatusErrors={$workspaceGitStatusErrors}
            workspaces={$workspaces}
            metadataItems={selectedSessionMetadataItems}
            metadataSummary={selectedSessionMetadataSummary}
            {inboxActionableCount}
            {submitting}
            {actionBusy}
            {canSend}
            onOpenInbox={() => (inboxSheetOpen = true)}
            onExit={() => void runSessionLifecycle('exit')}
            onOpenConsole={openSessionConsole}
            onNewChat={() => openNewChat(selectedSession.workspace_id)}
            onRename={openRenameSelectedSessionDialog}
            onRestart={() => void runSessionLifecycle('restart')}
            onSend={() => void sendMessage()}
            onFocus={() => void refreshCurrentSessionGitStatus()}
          />
        {/if}
      </div>
    </div>
  {/if}
</section>

<RenameSessionDialog
  bind:open={renameSessionDialogOpen}
  session={selectedSession}
  busy={actionBusy}
  error={actionError}
  onConfirm={(title) => void renameSelectedSession(title)}
  onCancel={() => (actionError = null)}
/>

<InboxSheet
  bind:open={inboxSheetOpen}
  {inboxActionableCount}
  {visibleInboxMessages}
  busyMessageId={inboxActionMessageId}
  onCancel={(message) => void cancelPendingInboxMessage(message)}
  onRetry={(message) => void retryFailedInboxMessage(message)}
  onDismiss={(message) => void dismissFailedInboxMessage(message)}
/>
