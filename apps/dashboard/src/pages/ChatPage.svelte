<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { toast } from 'svelte-sonner'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import type { DashboardStreamEvent, InboxMessageView, SessionView } from '../api/types'
  import {
    canSendSessionMessage,
    timelineItemsToChatMessages,
    titleFromInitialPrompt,
  } from '$lib/session-chat/sessionChat'
  import {
    chatMessagesWithOptimistic,
    optimisticInitialMessages,
    rememberOptimisticInitialMessage,
  } from '../stores/optimisticChat'
  import {
    clientTypeOptionsForProfile,
    defaultHandleForProfile,
    loadAgentProfiles,
    sessionProfileFields,
    agentProfiles,
    agentProfilesError,
    agentProfilesLoading,
  } from '../stores/agentProfiles'
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
    createDagTask,
    loadTaskProposals,
    taskProposals,
    taskProposalsError,
    taskProposalsLoading,
  } from '../stores/tasks'
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
  import { sessionMetadataItems, sessionMetadataSummary, visibleChatInboxMessages } from '../components/chat/sessionMetadata'

  let selectedSessionId = ''
  let prompt = ''
  let createWorkspaceId = ''
  let createProfileId = ''
  let createClientType = 'pi'
  let taskMode = false
  let creating = false

  let input = ''
  let submitting = false
  let actionBusy = false
  let inboxActionMessageId: string | null = null
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let inboxSheetOpen = false
  let loadedProposalTaskId = ''
  let appliedRedirectTaskId = ''
  let unsubscribeDashboardEvents: (() => void) | null = null
  let foregroundRefreshInFlight: Promise<void> | null = null


  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const DAG_TASK_ENTRIES_ENABLED = false
  const LAST_NEW_CHAT_WORKSPACE_STORAGE_KEY = 'pontia.chat.lastWorkspaceId'
  const newChatSelectorTriggerClass = 'h-7 rounded-full px-3 text-sm font-normal text-muted-foreground'

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces(), loadAgentProfiles()])
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

  $: selectedSession = selectedSessionId ? ($sessions.find((session) => session.session_id === selectedSessionId) ?? $sessionDetail?.session ?? null) : null
  $: selectedSessionGitStatus = selectedSession ? $workspaceGitStatuses[selectedSession.workspace_id ?? ''] : undefined
  $: selectedSessionMetadataItems = selectedSession ? sessionMetadataItems(selectedSession, $workspaces, selectedSessionGitStatus, $workspaceGitStatusErrors) : []
  $: selectedSessionMetadataSummary = sessionMetadataSummary(selectedSessionMetadataItems)
  $: messages = chatMessagesWithOptimistic(selectedSessionId, $timelineState.sessionId === selectedSessionId ? timelineItemsToChatMessages($timelineState.items) : [], $optimisticInitialMessages)
  $: selectedInboxMessages = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.inboxMessages : []
  $: visibleInboxMessages = visibleChatInboxMessages(selectedInboxMessages)
  $: inboxActionableCount = visibleInboxMessages.filter((message) => message.state === 'pending' || message.state === 'failed').length
  $: if ($workspaces.length && (!createWorkspaceId || !$workspaces.some((workspace) => workspace.workspace_id === createWorkspaceId))) {
    createWorkspaceId = preferredCreateWorkspaceId()
  }
  $: selectedProfile = $agentProfiles.find((profile) => profile.profile_id === createProfileId) ?? null
  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: clientTypeOptions = clientTypeOptionsForProfile(selectedProfile)
  $: if (!clientTypeOptions.includes(createClientType)) createClientType = clientTypeOptions[0] ?? createClientType
  $: if (createWorkspaceId && $workspaces.length) rememberCreateWorkspaceSelection(createWorkspaceId)
  $: canCreate = Boolean(prompt.trim() && createWorkspaceId && createClientType.trim() && !creating)
  $: canSend = canSendSessionMessage(selectedSession, input) && !submitting
  $: plannerTaskId = plannerTaskIdForSession(selectedSession)
  $: plannerTaskProposals = plannerTaskId ? $taskProposals.filter((proposal) => proposal.task_id === plannerTaskId) : []
  $: draftPlannerProposal = plannerTaskProposals.find((proposal) => proposal.mode === 'initial_dag' && proposal.state === 'proposed') ?? null
  $: if (DAG_TASK_ENTRIES_ENABLED && plannerTaskId && plannerTaskId !== loadedProposalTaskId) {
    loadedProposalTaskId = plannerTaskId
    void loadTaskProposals(plannerTaskId)
  }
  $: if (DAG_TASK_ENTRIES_ENABLED && plannerTaskId && plannerTaskProposals.some((proposal) => proposal.state === 'applied')) navigateToTaskDag(plannerTaskId)
  $: passiveErrorMessage = $sessionDetailError ?? $timelineState.error ?? $sessionsError ?? $workspacesError ?? $agentProfilesError ?? $taskProposalsError
  $: errorMessage = actionError ?? passiveErrorMessage
  $: shouldToastError = Boolean(actionError || (passiveErrorMessage && !isTransientFetchError(passiveErrorMessage)))
  $: {
    if (errorMessage && shouldToastError && errorMessage !== lastToastedError) {
      toast.error('Chat error', { description: errorMessage })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }

  function isTransientFetchError(message: string): boolean {
    return /fetch|network|load failed/i.test(message)
  }

  function requestedSessionIdFromLocation(): string {
    return getPathParams().sessionId ?? new URLSearchParams(window.location.search).get('session') ?? ''
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
    const rememberedWorkspaceId = readRememberedWorkspaceId()
    if (rememberedWorkspaceId && $workspaces.some((workspace) => workspace.workspace_id === rememberedWorkspaceId)) return rememberedWorkspaceId
    return $workspaces[0]?.workspace_id ?? ''
  }

  function ensureCreateWorkspaceSelection(): void {
    if (!$workspaces.length) return
    if (createWorkspaceId && $workspaces.some((workspace) => workspace.workspace_id === createWorkspaceId)) return
    createWorkspaceId = preferredCreateWorkspaceId()
  }

  function currentSelectedSession(): SessionView | null {
    if (!selectedSessionId) return null
    const detail = get(sessionDetail)
    if (detail?.session.session_id === selectedSessionId) return detail.session
    return get(sessions).find((session) => session.session_id === selectedSessionId) ?? null
  }

  async function refreshSessionGitStatus(session: SessionView | null): Promise<void> {
    if (!session?.workspace_id) return
    await refreshWorkspaceGitStatus(session.workspace_id)
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

  function plannerTaskIdForSession(session: SessionView | null): string | null {
    if (!session?.metadata) return null
    const metadata = session.metadata
    const taskId = typeof metadata.task_id === 'string' ? metadata.task_id : null
    const role = typeof metadata.dag_planning_role === 'string' ? metadata.dag_planning_role : null
    return metadata.dag_managed === true && role === 'planner' && taskId ? taskId : null
  }

  function navigateToTaskDag(taskId: string): void {
    if (appliedRedirectTaskId === taskId) return
    appliedRedirectTaskId = taskId
    navigate(`/tasks/${taskId}/dag`)
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

    if (!DAG_TASK_ENTRIES_ENABLED || !plannerTaskId || streamEvent.kind !== 'task_event') return
    if (streamEvent.event.task_id === plannerTaskId && streamEvent.event.event_type === 'dag.approved') {
      navigateToTaskDag(plannerTaskId)
    }
  }

  async function renameSelectedSession(): Promise<void> {
    if (!selectedSessionId || !selectedSession) return
    const nextTitle = window.prompt('Rename session', selectedSession.title ?? '')
    if (nextTitle === null) return
    actionBusy = true
    actionError = null
    try {
      await updateSessionTitle(selectedSessionId, nextTitle.trim() || null)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  function openSessionConsole(): void {
    navigate(selectedSessionId ? `/sessions/${selectedSessionId}` : '/sessions')
  }

  function openNewChat(): void {
    selectedSessionId = ''
    actionError = null
    resetTimelineState()
    navigate('/chat')
  }

  function applyProfileDefaults(): void {
    if (!selectedProfile) return
    createClientType = clientTypeOptionsForProfile(selectedProfile)[0] ?? createClientType
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
    }
  }

  function timelineRefreshMode(sessionId: string): 'rebuild' | 'append' {
    const currentTimeline = get(timelineState)
    return currentTimeline.sessionId === sessionId && currentTimeline.items.length ? 'append' : 'rebuild'
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    const mode = timelineRefreshMode(sessionId)
    if (mode === 'rebuild') resetTimelineState(sessionId)
    await Promise.all([loadSessionDetail(sessionId), loadSessionTimeline(sessionId, { mode })])
  }

  function handleNewChatKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void startChat()
    }
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      if (DAG_TASK_ENTRIES_ENABLED && taskMode) {
        const initialPrompt = prompt.trim()
        const result = await createDagTask({
          input: initialPrompt,
          workspace: selectedWorkspace?.canonical_path ?? selectedWorkspace?.display_path ?? createWorkspaceId,
          client_type: createClientType.trim() || 'pi',
          metadata: { source: 'dashboard_chat', action: 'manual_task' },
        })
        selectedSessionId = result.planning_turn.session_id
        rememberOptimisticInitialMessage(result.planning_turn.session_id, initialPrompt, {
          turn_id: result.planning_turn.turn_id,
          created_at: new Date().toISOString(),
        })
        prompt = ''
        resetTimelineState(result.planning_turn.session_id)
        navigate(`/chat/${result.planning_turn.session_id}`)
        await Promise.all([loadSessionDetail(result.planning_turn.session_id), loadSessionTimeline(result.planning_turn.session_id, { mode: 'rebuild' })])
        await refreshSessionGitStatus(currentSelectedSession())
        return
      }

      const initialPrompt = prompt.trim()
      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        handle: defaultHandleForProfile(selectedProfile) || null,
        role: selectedProfile?.default_session_role ?? null,
        title: titleFromInitialPrompt(initialPrompt),
        description: selectedProfile?.default_session_description ?? null,
        ...sessionProfileFields(selectedProfile),
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

<section class={selectedSessionId ? 'flex flex-col gap-4 pb-40' : 'flex min-h-[calc(100vh-9.5rem)] flex-col'}>
  {#if !selectedSessionId}
    <NewChatPanel
      bind:prompt
      bind:workspaceId={createWorkspaceId}
      bind:clientType={createClientType}
      bind:taskMode
      taskEntriesEnabled={DAG_TASK_ENTRIES_ENABLED}
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
    <div class="flex-1">
      <div class="flex flex-col rounded-xl bg-transparent">
        {#if $sessionDetailLoading && !selectedSession}
          <div class="space-y-4 p-6"><Skeleton class="h-10 w-1/3" /><Skeleton class="h-80 w-full" /></div>
        {:else if !selectedSession}
          <Empty.Root class="h-full">
            <Empty.Header>
              <Empty.Title>Session not found</Empty.Title>
              <Empty.Description>Start a new chat or select a recent session from the sidebar.</Empty.Description>
            </Empty.Header>
            <Empty.Content><Button onclick={openNewChat}>Start a new chat</Button></Empty.Content>
          </Empty.Root>
        {:else}
          <SessionConversation
            {messages}
            sessionState={selectedSession.state}
            loading={($sessionDetailLoading || $timelineState.loading) && !messages.length}
            plannerTaskId={DAG_TASK_ENTRIES_ENABLED ? plannerTaskId : null}
            draftPlannerProposal={DAG_TASK_ENTRIES_ENABLED ? draftPlannerProposal : null}
            draftPlannerProposalLoading={DAG_TASK_ENTRIES_ENABLED && $taskProposalsLoading}
            interruptEnabled={selectedSession.state === 'busy' && selectedSession.capabilities.interrupt === true}
            interruptBusy={actionBusy}
            hasMoreHistory={$timelineState.hasMore}
            historyLoading={$timelineState.refreshing}
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
            onRename={() => void renameSelectedSession()}
            onRestart={() => void runSessionLifecycle('restart')}
            onSend={() => void sendMessage()}
            onFocus={() => void refreshCurrentSessionGitStatus()}
          />
        {/if}
      </div>
    </div>
  {/if}
</section>

<InboxSheet
  bind:open={inboxSheetOpen}
  {inboxActionableCount}
  {visibleInboxMessages}
  busyMessageId={inboxActionMessageId}
  onCancel={(message) => void cancelPendingInboxMessage(message)}
  onRetry={(message) => void retryFailedInboxMessage(message)}
  onDismiss={(message) => void dismissFailedInboxMessage(message)}
/>
