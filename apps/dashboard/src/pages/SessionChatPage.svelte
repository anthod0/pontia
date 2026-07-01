<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import { get } from 'svelte/store'
  import { toast } from 'svelte-sonner'
  import { ChevronDown } from '@lucide/svelte'
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
    turnsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import {
    chatMessagesWithOptimistic,
    optimisticInitialMessages,
  } from '../stores/optimisticChat'
  import { chatDraft, clearChatDraft } from '../stores/chatDraft'
  import {
    loadWorkspaces,
    refreshWorkspaceGitStatus,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    workspaces,
    workspacesError,
  } from '../stores/workspaces'
  import {
    cancelInboxMessage,
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
  import SessionComposerDock from '../components/chat/SessionComposerDock.svelte'
  import { scrollDocumentToBottom } from '../lib/session-chat/autoScroll'
  import InboxSheet from '../components/chat/InboxSheet.svelte'
  import RenameSessionDialog from '../components/chat/RenameSessionDialog.svelte'
  import { sessionMetadataItems, sessionMetadataSummary, visibleChatInboxMessages } from '../components/chat/sessionMetadata'

  let selectedSessionId = ''
  let submitting = false
  let actionBusy = false
  let inboxActionMessageId: string | null = null
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let inboxSheetOpen = false
  let renameSessionDialogOpen = false
  let unsubscribeDashboardEvents: (() => void) | null = null
  let foregroundRefreshInFlight: Promise<void> | null = null
  let showScrollDownButton = false
  let scrollDownButtonRendered = false
  let scrollDownButtonHideTimer: ReturnType<typeof setTimeout> | null = null
  let bottomIntersectionObserver: IntersectionObserver | null = null
  let promptInputScrollBaselineKey: string | null = null
  let historyObserverEnabled = false
  let destroyed = false

  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const SCROLL_DOWN_BUTTON_ANIMATION_MS = 200
  const INITIAL_SCROLL_SETTLE_PASSES = 2

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces()])
    if (selectedSessionId) {
      await loadSelectedSession(selectedSessionId)
      await refreshSessionGitStatus(currentSelectedSession())
    }
    if (destroyed) return
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
    window.addEventListener('focus', handleForegroundResume)
    window.addEventListener('pageshow', handleForegroundResume)
    document.addEventListener('visibilitychange', handleVisibilityChange)
  })

  onDestroy(() => {
    destroyed = true
    unsubscribeDashboardEvents?.()
    window.removeEventListener('focus', handleForegroundResume)
    window.removeEventListener('pageshow', handleForegroundResume)
    document.removeEventListener('visibilitychange', handleVisibilityChange)
    bottomIntersectionObserver?.disconnect()
    if (scrollDownButtonHideTimer) clearTimeout(scrollDownButtonHideTimer)
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
  $: canSend = canSendSessionMessage(selectedSession, $chatDraft) && !submitting
  $: rawPassiveErrorMessage = $sessionDetailError ?? $timelineState.error ?? $sessionsError ?? $workspacesError
  $: passiveErrorMessage = rawPassiveErrorMessage && !isTransientNetworkError(rawPassiveErrorMessage) ? rawPassiveErrorMessage : null
  $: errorMessage = actionError ?? passiveErrorMessage
  $: {
    if (errorMessage && errorMessage !== lastToastedError) {
      toast.error('Chat error', { description: errorMessage })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }
  $: currentMessagesRenderKey = chatMessagesRenderKey(messages)
  $: if (promptInputScrollBaselineKey !== null && currentMessagesRenderKey !== promptInputScrollBaselineKey) {
    promptInputScrollBaselineKey = null
    void tick().then(scrollChatToBottom)
  }

  function requestedSessionIdFromLocation(): string {
    const routeSessionId = getPathParams().sessionId
    if (routeSessionId) return routeSessionId
    const pathMatch = window.location.pathname.match(/\/chat\/([^/?#]+)$/)
    return pathMatch ? decodeURIComponent(pathMatch[1]) : ''
  }

  function availableWorkspaceId(workspaceId: string | null): string | null {
    if (!workspaceId) return null
    return $workspaces.some((workspace) => workspace.workspace_id === workspaceId) ? workspaceId : null
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

  function chatMessagesRenderKey(chatMessages: typeof messages): string {
    return chatMessages.map((message) => [message.id, message.status, message.content].join('\u001f')).join('\u001e')
  }


  function setScrollDownButtonVisible(visible: boolean): void {
    if (scrollDownButtonHideTimer) {
      clearTimeout(scrollDownButtonHideTimer)
      scrollDownButtonHideTimer = null
    }

    if (visible) {
      scrollDownButtonRendered = true
      showScrollDownButton = true
      return
    }

    showScrollDownButton = false
    scrollDownButtonHideTimer = setTimeout(() => {
      if (!showScrollDownButton) scrollDownButtonRendered = false
      scrollDownButtonHideTimer = null
    }, SCROLL_DOWN_BUTTON_ANIMATION_MS)
  }

  function observeBottomSentinel(node: HTMLElement): { destroy: () => void } {
    bottomIntersectionObserver?.disconnect()
    if (typeof IntersectionObserver === 'undefined') return { destroy: () => undefined }
    const observer = new IntersectionObserver((entries) => {
      const entry = entries[0]
      if (!entry) return
      setScrollDownButtonVisible(!entry.isIntersecting)
    }, { threshold: 0.01 })
    bottomIntersectionObserver = observer
    observer.observe(node)
    return {
      destroy: () => {
        observer.disconnect()
        if (bottomIntersectionObserver === observer) bottomIntersectionObserver = null
      },
    }
  }

  function nextAnimationFrame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()))
  }

  function scrollChatToBottom(): void {
    scrollDocumentToBottom()
    setScrollDownButtonVisible(false)
  }

  async function scrollChatToBottomAfterLayout(): Promise<void> {
    await tick()
    await nextAnimationFrame()
    for (let pass = 0; pass < INITIAL_SCROLL_SETTLE_PASSES; pass += 1) {
      scrollChatToBottom()
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
    actionError = null
    resetTimelineState()
    if (queryWorkspaceId) {
      const availableQueryWorkspaceId = availableWorkspaceId(queryWorkspaceId)
      navigate('/chat', { workspace: availableQueryWorkspaceId ?? queryWorkspaceId })
      return
    }
    navigate('/chat')
  }

  async function selectSessionFromLocation(): Promise<void> {
    const nextSessionId = requestedSessionIdFromLocation()
    if (nextSessionId === selectedSessionId) return
    selectedSessionId = nextSessionId
    actionError = null
    if (selectedSessionId) {
      await loadSelectedSession(selectedSessionId)
      await refreshSessionGitStatus(currentSelectedSession())
    } else {
      historyObserverEnabled = false
      resetTimelineState()
    }
  }

  function sessionSupportsTimeline(session: SessionView | null): boolean {
    return session?.capabilities.timeline === true
  }

  function redirectToSessionDetail(sessionId: string): void {
    historyObserverEnabled = false
    resetTimelineState(sessionId)
    navigate(`/sessions/${sessionId}`)
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    historyObserverEnabled = false
    await loadSessionDetail(sessionId)
    const loadedSession = currentSelectedSession()
    if (loadedSession && !sessionSupportsTimeline(loadedSession)) {
      redirectToSessionDetail(sessionId)
      return
    }

    const currentTimeline = get(timelineState)
    const hasLoadedTimeline = currentTimeline.sessionId === sessionId && currentTimeline.items.length > 0
    if (!hasLoadedTimeline) resetTimelineState(sessionId)
    if (hasLoadedTimeline) await handleTimelineMessageUpdated(sessionId)
    else await loadSessionTimeline(sessionId, { mode: 'rebuild' })
    await scrollChatToBottomAfterLayout()
    if (!destroyed && selectedSessionId === sessionId) historyObserverEnabled = true
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
    const message = $chatDraft.trim()
    promptInputScrollBaselineKey = chatMessagesRenderKey(messages)
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
      clearChatDraft()
      await tick()
      scrollChatToBottom()
    } catch (error) {
      promptInputScrollBaselineKey = null
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submitting = false
    }
  }
</script>

<svelte:window onpopstate={() => void selectSessionFromLocation()} />

<section class="flex flex-col gap-4 pb-40">
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
        {#key selectedSessionId}
          <SessionConversation
            {messages}
            sessionState={selectedSession.state}
            loading={($sessionDetailLoading || $timelineState.loading) && !messages.length}
            interruptEnabled={selectedSession.state === 'busy' && selectedSession.capabilities.interrupt === true}
            interruptBusy={actionBusy}
            hasMoreHistory={$timelineState.hasMore}
            historyLoading={$timelineState.refreshKind === 'history'}
            {historyObserverEnabled}
            onInterrupt={() => void interruptSelectedSession()}
            onLoadMoreHistory={loadEarlierMessages}
          />
        {/key}
        <div aria-hidden="true" class="h-px w-px" data-chat-bottom-sentinel use:observeBottomSentinel></div>

        {#if scrollDownButtonRendered}
          <div
            data-chat-scroll-down-container
            class={`pointer-events-none fixed bottom-36 left-0 right-0 z-40 px-2 transition-[left] duration-200 ease-linear sm:px-4 md:left-[var(--sidebar-width)] md:px-6 group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)] ${showScrollDownButton ? 'chat-scroll-down-enter' : 'chat-scroll-down-exit'}`}
          >
            <div class="mx-auto flex w-full max-w-4xl justify-end">
              <Button
                type="button"
                variant="secondary"
                size="icon"
                class="pointer-events-auto rounded-full shadow-lg"
                aria-label="Scroll to bottom"
                title="Scroll to bottom"
                onclick={scrollChatToBottom}
              >
                <ChevronDown class="size-4" />
              </Button>
            </div>
          </div>
        {/if}

        <SessionComposerDock
          bind:input={$chatDraft}
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

<style>
  :global([data-chat-scroll-down-container].chat-scroll-down-enter) {
    animation: chat-scroll-down-in 180ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  :global([data-chat-scroll-down-container].chat-scroll-down-exit) {
    animation: chat-scroll-down-out 160ms cubic-bezier(0.4, 0, 1, 1) both;
  }

  @keyframes chat-scroll-down-in {
    from {
      opacity: 0;
      translate: 0 0.75rem;
    }
    to {
      opacity: 1;
      translate: 0 0;
    }
  }

  @keyframes chat-scroll-down-out {
    from {
      opacity: 1;
      translate: 0 0;
    }
    to {
      opacity: 0;
      translate: 0 0.75rem;
    }
  }
</style>
