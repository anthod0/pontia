<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import { get } from 'svelte/store'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { navigate } from '$lib/navigation'
  import { getSession, openCodexTui } from '../api/client'
  import { claimChatEntryAutofocus } from '$lib/chatEntryAutofocus'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import ChatRuler from '$lib/components/session-chat/ChatRuler.svelte'
  import type { DashboardStreamEvent, InboxMessageView, SessionView, TurnView } from '../api/types'
  import type { ChatMessageRole, SessionChatMessage } from '$lib/session-chat/sessionChat'
  import {
    canSendSessionMessage,
    sessionChatTitle,
    timelineItemsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import type { LiveOutputEvent, LiveOutputOverlays } from '$lib/session-chat/liveOutput'
  import {
    applyLiveOutputEvent,
    markLiveOutputDisconnected,
    mergeLiveOutputMessages,
    removeLiveOutputOverlay,
  } from '$lib/session-chat/liveOutput'
  import {
    chatMessagesWithOptimistic,
    optimisticInitialMessages,
    reconcileOptimisticMessages,
  } from '../stores/optimisticChat'
  import {
    consumeInboxSubmission,
    inboxSubmissionMessages,
    optimisticInboxSubmissions,
    reconcileInboxSubmissions,
  } from '../stores/optimisticInbox'
  import { chatDraft, clearChatDraft } from '../stores/chatDraft'
  import {
    loadWorkspaces,
    refreshWorkspaceGitStatus,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    workspaces,
  } from '../stores/workspaces'
  import {
    cancelInboxMessage,
    dismissInboxMessage,
    loadSessionDetail,
    loadSessions,
    interruptSession,
    terminateSession,
    resumeSession,
    sessionDetail,
    sessionDetailLoading,
    sessions,
    submitInboxMessage,
    updateSessionTitle,
  } from '../stores/sessions'
  import {
    hasTimelineSnapshot,
    loadSessionTimeline,
    refreshSessionTimeline,
    resetTimelineState,
    restoreSessionTimeline,
    timelineState,
  } from '../stores/timeline'
  import { subscribeDashboardEvents } from '../services/eventStream'
  import { openLiveOutputStream } from '../services/liveOutputStream'
  import SessionComposerDock from '../components/chat/SessionComposerDock.svelte'
  import { scrollDocumentToBottom } from '../lib/session-chat/autoScroll'
  import { sessionMetadataItems, sessionMetadataSummary, visibleChatInboxMessages } from '../components/chat/sessionMetadata'
  import { isTerminalSession } from './sessions/sessionList'

  export let routeSessionId: string | null = null

  let selectedSessionId = ''
  let composerHeight = 180
  let submitting = false
  let branchActionSubmitting = false
  let branchActionError: string | null = null
  let actionBusy = false
  let inboxActionMessageId: string | null = null
  let actionError: string | null = null
  let unsubscribeDashboardEvents: (() => void) | null = null
  let closeLiveOutputStream: (() => void) | null = null
  let liveOutputOverlays: LiveOutputOverlays = {}
  let autofocusComposer = false
  let showScrollDownButton = false
  let scrollDownButtonRendered = false
  let scrollDownButtonHideTimer: ReturnType<typeof setTimeout> | null = null
  let bottomIntersectionObserver: IntersectionObserver | null = null
  interface PromptScrollRequest {
    sessionId: string
    input: string
    turnIds: Set<string>
  }
  let pendingPromptScrolls: PromptScrollRequest[] = []
  let promptScrollScheduled = false
  let historyObserverEnabled = false
  let initialChatScrollPending = false
  let destroyed = false

  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const BRANCH_INTERRUPT_TIMEOUT_MS = 30_000
  const SCROLL_DOWN_BUTTON_ANIMATION_MS = 200
  const INITIAL_SCROLL_SETTLE_PASSES = 2

  let codexPoll: ReturnType<typeof setInterval> | null = null
  let codexRefreshing = false
  async function refreshCodex(): Promise<void> {
    if (codexRefreshing || selectedSession?.client_type !== 'codex') return
    codexRefreshing = true
    const id = selectedSessionId
    try {
      const detail = await loadSessionDetail(id, { showLoading: false })
      if (destroyed || selectedSessionId !== id) return
      const owner = new URLSearchParams(window.location.search).get('tui')
      const tui = owner
        ? (owner === id ? detail?.session.codex?.owned_tui : (await getSession(owner)).codex?.owned_tui)
        : detail?.session.codex?.tui
      if (destroyed || selectedSessionId !== id) return
      if (tui?.connected && tui.target_session_id !== id) {
        await navigate(`/chat/${tui.target_session_id}`, { tui: tui.owner_session_id })
      }
      if (detail?.session.capabilities.timeline && !hasTimelineSnapshot(get(timelineState), id)) {
        await loadSessionTimeline(id)
      }
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally { codexRefreshing = false }
  }
  async function openSelectedTui(): Promise<void> {
    if (!selectedSessionId || actionBusy) return
    actionBusy = true
    try { await openCodexTui(selectedSessionId); await refreshCodex() }
    catch (error) { actionError = error instanceof Error ? error.message : String(error) }
    finally { actionBusy = false }
  }

  onMount(async () => {
    codexPoll = setInterval(() => void refreshCodex(), 2000)
    selectedSessionId = requestedSessionIdFromLocation()
    autofocusComposer = claimChatEntryAutofocus(`/chat/${selectedSessionId}`)
    initialChatScrollPending = Boolean(selectedSessionId)
    await Promise.all([loadSessions(), loadWorkspaces()])
    if (selectedSessionId) await loadSelectedSession(selectedSessionId)
    if (destroyed) return
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
  })

  onDestroy(() => {
    destroyed = true
    if (codexPoll) clearInterval(codexPoll)
    unsubscribeDashboardEvents?.()
    closeLiveOutputStream?.()
    bottomIntersectionObserver?.disconnect()
    if (scrollDownButtonHideTimer) clearTimeout(scrollDownButtonHideTimer)
  })

  $: selectedSession = selectedSessionId ? ($sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.session : $sessions.find((session) => session.session_id === selectedSessionId) ?? null) : null
  $: selectedSessionGitStatus = selectedSession ? $workspaceGitStatuses[selectedSession.workspace_id ?? ''] : undefined
  $: selectedSessionMetadataItems = selectedSession ? sessionMetadataItems(selectedSession, $workspaces, selectedSessionGitStatus, $workspaceGitStatusErrors) : []
  $: selectedSessionMetadataSummary = sessionMetadataSummary(selectedSessionMetadataItems)
  $: transcriptMessages = $timelineState.sessionId === selectedSessionId
    ? timelineItemsToChatMessages($timelineState.items, $timelineState.mode === 'tree')
    : []
  $: selectedTurns = $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.turns : []
  $: timelineMessages = mergeLiveOutputMessages(
    transcriptMessages,
    selectedTurns,
    liveOutputOverlays,
    selectedSession?.current_turn_id ?? null,
  )
  $: reconcileOptimisticMessages(selectedSessionId, timelineMessages)
  $: reconcileInboxSubmissions(selectedSessionId, timelineMessages)
  $: messages = inboxSubmissionMessages(
    selectedSessionId,
    chatMessagesWithOptimistic(selectedSessionId, timelineMessages, $optimisticInitialMessages),
    $optimisticInboxSubmissions,
  )
  $: branchActionInputs = eligibleBranchActionInputs(selectedSession, messages)
  $: branchActionMessageIds = Object.keys(branchActionInputs)
  $: timelineUnavailable = $timelineState.sessionId === selectedSessionId && Boolean($timelineState.error)
  $: rulerTurns = $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.turns : []
  $: rulerTreeMode = $timelineState.sessionId === selectedSessionId && $timelineState.mode === 'tree'
  $: rulerNavigableTurnIds = navigableRulerTurnIds(
    rulerTurns,
    rulerTreeMode,
    $timelineState.sessionId === selectedSessionId ? $timelineState.latestTurnId : null,
  )
  $: selectedInboxMessages = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.inboxMessages : []
  $: visibleInboxMessages = visibleChatInboxMessages(selectedInboxMessages)
  $: canSend = canSendSessionMessage(selectedSession, $chatDraft) && !submitting
  $: if (!timelineUnavailable && matchingPromptScrolls(messages, pendingPromptScrolls, selectedSessionId).length) {
    void scrollSubmittedPromptAfterLayout(selectedSessionId)
  }

  function requestedSessionIdFromLocation(): string {
    if (routeSessionId) return routeSessionId
    const pathMatch = window.location.pathname.match(/\/chat\/([^/?#]+)$/)
    return pathMatch ? decodeURIComponent(pathMatch[1]) : ''
  }

  function eligibleBranchActionInputs(
    session: SessionView | null,
    chatMessages: typeof messages,
  ): Record<string, string> {
    if (!session?.capabilities.branch_control) return {}
    const canEditInCurrentState = ['idle', 'interrupted', 'exited'].includes(session.state)
      || (session.state === 'busy' && session.capabilities.interrupt === true)
    if (!canEditInCurrentState) return {}
    const turns = $sessionDetail?.session.session_id === session.session_id
      ? new Map($sessionDetail.turns.map((turn) => [turn.turn_id, turn]))
      : new Map()
    const seenTurnIds = new Set<string>()
    const eligibleTurnStates = new Set(['completed', 'failed', 'interrupted', 'abandoned'])

    return Object.fromEntries(chatMessages.flatMap((message) => {
      if (message.role !== 'user' || message.status !== 'sent' || seenTurnIds.has(message.turnId)) return []
      seenTurnIds.add(message.turnId)
      const projectedTurn = turns.get(message.turnId)
      if (!projectedTurn || !eligibleTurnStates.has(projectedTurn.state)) return []
      if (!projectedTurn.input?.summary?.trim() || !message.content.trim()) return []
      return [[message.id, message.content]]
    }))
  }

  function navigableRulerTurnIds(
    turns: TurnView[],
    treeMode: boolean,
    latestTurnId: string | null,
  ): string[] {
    if (!treeMode) return turns.map((turn) => turn.turn_id)
    if (!latestTurnId) return []

    const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]))
    const lineage: string[] = []
    const visited = new Set<string>()
    let turnId: string | null = latestTurnId
    while (turnId && !visited.has(turnId)) {
      visited.add(turnId)
      lineage.push(turnId)
      turnId = turnsById.get(turnId)?.parent_turn_id ?? null
    }
    return lineage
  }

  function chatMessageElement(turnId: string, role: ChatMessageRole): HTMLElement | null {
    return [...document.querySelectorAll<HTMLElement>('[data-chat-message-id][data-chat-turn-id][data-chat-role]')]
      .find((element) => element.dataset.chatTurnId === turnId && element.dataset.chatRole === role) ?? null
  }

  async function navigateFromRuler(turnId: string, role: ChatMessageRole): Promise<void> {
    if (!rulerNavigableTurnIds.includes(turnId)) return
    let target = chatMessageElement(turnId, role)
    const visitedCursors = new Set<string | null>()

    while (!target) {
      const state = get(timelineState)
      if (state.sessionId !== selectedSessionId || !state.hasMore || state.refreshing) return
      if (visitedCursors.has(state.nextOlderTurnId)) return
      visitedCursors.add(state.nextOlderTurnId)
      await loadSessionTimeline(selectedSessionId, {
        mode: 'more',
        ...(rulerTreeMode ? { topology: true } : {}),
      })
      await tick()
      target = chatMessageElement(turnId, role)
    }

    target.scrollIntoView({ behavior: 'smooth', block: 'start' })
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
        delivery_policy: message.delivery_policy === 'steer' ? 'steer' : message.delivery_policy === 'interrupt_now' ? 'interrupt_now' : 'after_idle',
        metadata: message.metadata,
        ...(message.branch_target_turn_id
          ? { branch_target_turn_id: message.branch_target_turn_id }
          : {}),
      }, { showInChat: false })
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

  function projectedTurnForBranchMessage(message: SessionChatMessage) {
    if (!branchActionMessageIds.includes(message.id)) return null
    return $sessionDetail?.turns.find((turn) => turn.turn_id === message.turnId) ?? null
  }

  async function submitBranchEdit(
    message: SessionChatMessage,
    input: string,
  ): Promise<boolean> {
    const projectedTurn = projectedTurnForBranchMessage(message)
    if (!selectedSessionId || !projectedTurn || !input.trim() || branchActionSubmitting) return false

    const sessionId = selectedSessionId
    branchActionSubmitting = true
    branchActionError = null
    try {
      const session = currentSelectedSession()
      if (session?.state === 'busy') {
        if (session.capabilities.interrupt !== true) {
          throw new Error('This Session cannot interrupt the active Turn before editing history.')
        }
        await interruptSession(sessionId)
        await waitForBranchSubmissionReady(sessionId)
      }
      await submitInboxMessage(sessionId, {
        input,
        delivery_policy: 'after_idle',
        metadata: { source: 'dashboard_chat_branch_edit' },
        branch_target_turn_id: projectedTurn.turn_id,
      })
      return true
    } catch (error) {
      branchActionError = error instanceof Error ? error.message : String(error)
      return false
    } finally {
      branchActionSubmitting = false
    }
  }

  function editHistoricalMessage(message: SessionChatMessage, replacementInput: string): Promise<boolean> {
    return submitBranchEdit(message, replacementInput)
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

  function matchingPromptScrolls(
    chatMessages: SessionChatMessage[],
    requests: PromptScrollRequest[],
    sessionId: string,
  ): PromptScrollRequest[] {
    const matchedTurnIds = new Set<string>()
    return requests.filter((request) => {
      if (request.sessionId !== sessionId) return false
      const message = chatMessages.find((item) => item.role === 'user'
        && !request.turnIds.has(item.turnId)
        && !matchedTurnIds.has(item.turnId)
        && item.content === request.input)
      if (!message) return false
      matchedTurnIds.add(message.turnId)
      return true
    })
  }

  async function scrollSubmittedPromptAfterLayout(sessionId: string): Promise<void> {
    if (promptScrollScheduled) return
    promptScrollScheduled = true
    await tick()
    await nextAnimationFrame()
    promptScrollScheduled = false
    if (destroyed || selectedSessionId !== sessionId) return
    const mountedMessageIds = new Set([...document.querySelectorAll<HTMLElement>('[data-chat-message-id][data-chat-role="user"]')]
      .map((element) => element.dataset.chatMessageId))
    const rendered = matchingPromptScrolls(messages.filter((message) => mountedMessageIds.has(message.id)), pendingPromptScrolls, sessionId)
    if (!rendered.length) return
    scrollChatToBottom()
    pendingPromptScrolls = pendingPromptScrolls.filter((request) => !rendered.includes(request))
    for (const request of pendingPromptScrolls) {
      for (const message of messages) request.turnIds.add(message.turnId)
    }
  }

  async function scrollChatToBottomAfterLayout(): Promise<void> {
    await tick()
    await nextAnimationFrame()
    for (let pass = 0; pass < INITIAL_SCROLL_SETTLE_PASSES; pass += 1) {
      scrollChatToBottom()
    }
  }

  function isTerminalTurnEvent(eventType: string): boolean {
    return eventType === 'turn.completed'
      || eventType === 'turn.failed'
      || eventType === 'turn.interrupted'
      || eventType === 'turn.abandoned'
  }

  function isSessionIdleEvent(eventType: string): boolean {
    return eventType === 'session.ready' || isTerminalTurnEvent(eventType)
  }

  async function convergeTerminalTurn(turnId: string): Promise<void> {
    const sessionId = selectedSessionId
    const [, timelineSucceeded] = await Promise.all([
      loadSessionDetail(sessionId, { showLoading: false }),
      refreshSessionTimeline(sessionId, turnId),
    ])
    if (!timelineSucceeded || selectedSessionId !== sessionId) return
    const timeline = get(timelineState)
    const detail = get(sessionDetail)
    const terminalTurn = detail?.session.session_id === sessionId
      ? detail.turns.find((turn) => turn.turn_id === turnId)
      : null
    if (!terminalTurn || !['completed', 'failed', 'interrupted', 'abandoned'].includes(terminalTurn.state)) return
    if (timeline.sessionId !== sessionId || !timeline.items.some((item) => item.turn_id === turnId)) return
    liveOutputOverlays = removeLiveOutputOverlay(liveOutputOverlays, turnId)
  }

  function handleLiveOutputEvent(event: LiveOutputEvent): void {
    liveOutputOverlays = applyLiveOutputEvent(liveOutputOverlays, selectedSessionId, event)
  }

  function startSelectedLiveOutput(session: SessionView | null): void {
    closeLiveOutputStream?.()
    closeLiveOutputStream = null
    if (!session || session.session_id !== selectedSessionId || session.capabilities.stream_output !== true) return
    closeLiveOutputStream = openLiveOutputStream(session.session_id, {
      onEvent: handleLiveOutputEvent,
      onDisconnected: () => {
        liveOutputOverlays = markLiveOutputDisconnected(liveOutputOverlays)
      },
    })
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === 'session_event') {
      if (streamEvent.event.session_id !== selectedSessionId) return
      const metadata = streamEvent.event.payload.metadata
      if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
        const inboxMessageId = (metadata as Record<string, unknown>).inbox_message_id
        if (typeof inboxMessageId === 'string') consumeInboxSubmission(inboxMessageId, streamEvent.event.session_id)
      }
      if (isTerminalTurnEvent(streamEvent.event.type) && streamEvent.event.turn_id) {
        void convergeTerminalTurn(streamEvent.event.turn_id)
        return
      }
      if (isSessionIdleEvent(streamEvent.event.type)) {
        void loadSessionDetail(selectedSessionId, { showLoading: false })
        void refreshSessionTimeline(selectedSessionId, streamEvent.event.turn_id)
        return
      }
      if (streamEvent.event.type === 'turn.started') {
        void loadSessionDetail(selectedSessionId, { showLoading: false })
        void refreshSessionTimeline(selectedSessionId, streamEvent.event.turn_id)
        return
      }
      if (streamEvent.event.type !== 'session.message_updated') return
      void refreshSessionTimeline(selectedSessionId, streamEvent.event.turn_id)
      return
    }
  }

  function latestProjectedTurnId(): string | null {
    if (!$sessionDetail || $sessionDetail.session.session_id !== selectedSessionId) return null
    return $sessionDetail.turns.reduce<string | null>(
      (latestTurnId, turn) => latestTurnId === null || turn.turn_id > latestTurnId
        ? turn.turn_id
        : latestTurnId,
      null,
    )
  }

  function openNewChat(): void {
    actionError = null
    clearChatDraft()
    resetTimelineState()
    navigate('/', { workspace: selectedSession?.workspace_id })
  }

  async function selectSessionFromLocation(): Promise<void> {
    const nextSessionId = requestedSessionIdFromLocation()
    if (nextSessionId === selectedSessionId) return
    closeLiveOutputStream?.()
    closeLiveOutputStream = null
    liveOutputOverlays = {}
    selectedSessionId = nextSessionId
    pendingPromptScrolls = []
    actionError = null
    branchActionError = null
    if (selectedSessionId) {
      autofocusComposer = claimChatEntryAutofocus(`/chat/${selectedSessionId}`)
      await loadSelectedSession(selectedSessionId)
    } else {
      historyObserverEnabled = false
      initialChatScrollPending = false
      resetTimelineState()
    }
  }

  function sessionSupportsTimeline(session: SessionView | null): boolean {
    return session?.capabilities.timeline === true
  }

  function redirectToSessionDetail(sessionId: string): void {
    historyObserverEnabled = false
    initialChatScrollPending = false
    resetTimelineState(sessionId)
    navigate(`/sessions/${sessionId}`)
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    historyObserverEnabled = false
    initialChatScrollPending = true
    try {
      await loadSessionDetail(sessionId)
      const loadedSession = currentSelectedSession()
      if (loadedSession?.client_type === 'codex' && !sessionSupportsTimeline(loadedSession)) {
        initialChatScrollPending = false
        resetTimelineState(sessionId)
        return
      }
      if (loadedSession && !sessionSupportsTimeline(loadedSession)) {
        redirectToSessionDetail(sessionId)
        return
      }
      startSelectedLiveOutput(loadedSession)

      let currentTimeline = get(timelineState)
      const latestTurnId = latestProjectedTurnId()
      const topology = loadedSession?.capabilities.topology === true
      const expectedMode = topology ? 'tree' : 'linear'
      let hasLoadedTimeline = hasTimelineSnapshot(currentTimeline, sessionId)
        && currentTimeline.mode === expectedMode
      if (!hasLoadedTimeline) {
        resetTimelineState(sessionId)
        await restoreSessionTimeline(sessionId, { topology })
        currentTimeline = get(timelineState)
        hasLoadedTimeline = hasTimelineSnapshot(currentTimeline, sessionId)
          && currentTimeline.mode === expectedMode
      }
      if (hasLoadedTimeline) void refreshSessionTimeline(sessionId, currentTimeline.latestTurnId ?? latestTurnId)
      else await loadSessionTimeline(sessionId, {
        mode: 'rebuild',
        latestTurnId,
        ...(topology ? { topology: true } : {}),
      })
      await scrollChatToBottomAfterLayout()
      if (!destroyed && selectedSessionId === sessionId) {
        initialChatScrollPending = false
        historyObserverEnabled = true
      }
    } catch (error) {
      if (!destroyed && selectedSessionId === sessionId) initialChatScrollPending = false
      throw error
    }
  }

  async function loadEarlierMessages(): Promise<void> {
    if (!selectedSessionId || !$timelineState.hasMore || $timelineState.refreshing) return
    actionError = null
    try {
      await loadSessionTimeline(selectedSessionId, {
        mode: 'more',
        ...(selectedSession?.capabilities.topology === true ? { topology: true } : {}),
      })
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    }
  }

  async function renameSelectedSession(title: string): Promise<void> {
    if (!selectedSession || actionBusy || !title.trim()) return
    const sessionId = selectedSessionId
    const commandInput = $chatDraft
    actionBusy = true
    actionError = null
    try {
      await updateSessionTitle(sessionId, title)
      if (selectedSessionId === sessionId && $chatDraft === commandInput) clearChatDraft()
    } catch (error) {
      if (selectedSessionId === sessionId) actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  async function exitSelectedSession(): Promise<void> {
    if (!selectedSession || isTerminalSession(selectedSession) || actionBusy) return
    const sessionId = selectedSessionId
    actionBusy = true
    actionError = null
    try {
      await terminateSession(sessionId)
      if (selectedSessionId === sessionId && $chatDraft.trim() === '/exit') clearChatDraft()
    } catch (error) {
      if (selectedSessionId === sessionId) actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  async function interruptSelectedSession(): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    try {
      await interruptSession(selectedSessionId)
      await refreshSessionTimeline(selectedSessionId, selectedSession?.current_turn_id ?? latestProjectedTurnId())
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

  function branchSubmissionReadyFromStores(sessionId: string): boolean {
    const detail = get(sessionDetail)
    if (detail?.session.session_id !== sessionId) return false
    if (!['idle', 'interrupted'].includes(detail.session.state)) return false
    return !detail.turns.some((turn) => turn.state === 'queued' || turn.state === 'running')
  }

  function waitForBranchSubmissionReady(sessionId: string, timeoutMs = BRANCH_INTERRUPT_TIMEOUT_MS): Promise<void> {
    if (branchSubmissionReadyFromStores(sessionId)) return Promise.resolve()

    return new Promise((resolve, reject) => {
      let done = false
      let unsubscribe: (() => void) | null = null

      const finish = (callback: () => void) => {
        if (done) return
        done = true
        unsubscribe?.()
        clearTimeout(timeout)
        callback()
      }
      const check = () => {
        if (branchSubmissionReadyFromStores(sessionId)) finish(resolve)
      }
      const timeout = setTimeout(() => {
        finish(() => reject(new Error('Edit timed out waiting for the active Turn to be interrupted.')))
      }, timeoutMs)

      const stop = sessionDetail.subscribe(check)
      unsubscribe = stop
      if (done) stop()
      else check()
    })
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
    const scrollRequest = {
      sessionId: selectedSessionId,
      input: message,
      turnIds: new Set([...messages.map((item) => item.turnId), ...selectedTurns.map((turn) => turn.turn_id)]),
    }
    pendingPromptScrolls = [...pendingPromptScrolls, scrollRequest]
    const waitForResume = selectedSession?.state === 'exited'
    if (!waitForResume) clearChatDraft()
    try {
      if (waitForResume) {
        await resumeSession(selectedSessionId)
        await waitForSessionIdle(selectedSessionId)
        clearChatDraft()
      }
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: selectedSession?.client_type === 'codex' ? 'steer' : 'after_idle',
        metadata: { source: 'dashboard_chat' },
      })
    } catch (error) {
      pendingPromptScrolls = pendingPromptScrolls.filter((request) => request !== scrollRequest)
      if (!get(chatDraft).trim()) chatDraft.set(message)
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submitting = false
    }
  }
</script>

<svelte:window onpopstate={() => void selectSessionFromLocation()} />

<section class="flex flex-col gap-4 pb-[var(--chat-bottom-padding)]" style={`--chat-top-offset: 4rem; --chat-bottom-padding: ${composerHeight + 16}px; --chat-composer-height: ${composerHeight}px`}>
  {#if selectedSession}
    <h1 class="truncate pt-1 text-base font-normal text-heading" title={sessionChatTitle(selectedSession)}>{sessionChatTitle(selectedSession)}</h1>
  {/if}
  {#if selectedSession?.codex}
    <div class="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
      <span>Control: {selectedSession.codex.connection.replaceAll('_', ' ')}</span>
      <span>TUI: {selectedSession.codex.tui?.connected ? 'connected' : 'disconnected'}</span>
      <Button variant="outline" size="sm" disabled={!selectedSession.codex.thread_id || actionBusy || selectedSession.state === 'exited'} onclick={() => void openSelectedTui()}>Open TUI</Button>
      {#if selectedSession.codex.tui?.pane_id}
        <code>tmux attach -t pontia_codex_{(selectedSession.codex.tui.owner_session_id ?? selectedSession.session_id).replaceAll('-', '_')}</code>
      {/if}
      {#if !selectedSession.capabilities.timeline}<span>Native history is currently unavailable.</span>{/if}
    </div>
  {/if}
  {#if actionError}
    <Alert.Root variant="destructive" role="alert" class="mx-auto w-full max-w-[760px]">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>Session action failed</Alert.Title>
      <Alert.Description>{actionError}</Alert.Description>
    </Alert.Root>
  {/if}
  <div class="mx-auto min-w-0 w-full max-w-[760px] flex-1">
    <div class="relative flex min-w-0 flex-col rounded-none bg-transparent">
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
        <div
          data-chat-initial-scroll-pending={initialChatScrollPending ? 'true' : 'false'}
          class={initialChatScrollPending ? 'relative min-h-80' : 'relative'}
        >
          {#if initialChatScrollPending}
            <div
              data-chat-conversation-skeleton
              class="pointer-events-none absolute inset-x-0 top-0 z-10 space-y-8 py-4 "
              role="status"
              aria-label="Loading conversation"
            >
              <div class="flex justify-end"><Skeleton class="h-14 w-3/5 max-w-xl rounded-none" /></div>
              <div class="w-4/5 max-w-2xl space-y-3">
                <Skeleton class="h-4 w-full" />
                <Skeleton class="h-4 w-11/12" />
                <Skeleton class="h-4 w-2/3" />
              </div>
              <div class="flex justify-end"><Skeleton class="h-10 w-2/5 max-w-md rounded-none" /></div>
              <div class="w-3/4 max-w-xl space-y-3">
                <Skeleton class="h-4 w-full" />
                <Skeleton class="h-4 w-4/5" />
              </div>
            </div>
          {/if}
          <div class={initialChatScrollPending ? 'opacity-0' : ''}>
            {#if timelineUnavailable}
              <Empty.Root data-timeline-status={$timelineState.status} class="min-h-80">
                <Empty.Header>
                  <Empty.Title>Conversation history unavailable</Empty.Title>
                  <Empty.Description>{$timelineState.error}</Empty.Description>
                </Empty.Header>
              </Empty.Root>
            {:else}
              {#key selectedSessionId}
                <SessionConversation
                  {messages}
                  sessionState={selectedSession.state}
                  activeTurnId={selectedSession.current_turn_id}
                  loading={(initialChatScrollPending || $sessionDetailLoading || $timelineState.loading) && !messages.length}
                  hasMoreHistory={$timelineState.hasMore}
                  historyLoading={$timelineState.refreshKind === 'history'}
                  {historyObserverEnabled}
                  {branchActionInputs}
                  branchActionBusy={branchActionSubmitting}
                  onBranchEdit={editHistoricalMessage}
                  onLoadMoreHistory={loadEarlierMessages}
                />
              {/key}
            {/if}
          </div>
        </div>
        {#if branchActionError}
          <Alert.Root variant="destructive" role="alert" class="mx-4 mb-4">
            <WarningCircleIcon class="size-4" />
            <Alert.Title>Branch action failed</Alert.Title>
            <Alert.Description>{branchActionError}</Alert.Description>
          </Alert.Root>
        {/if}
        <div aria-hidden="true" class="absolute bottom-0 h-px w-px" data-chat-bottom-sentinel use:observeBottomSentinel></div>

        {#if scrollDownButtonRendered}
          <div
            data-chat-scroll-down-container
            style="bottom: calc(var(--chat-composer-height) + 0.5rem)"
            class={`pointer-events-none fixed left-0 right-0 z-40 px-4 transition-[left] duration-200 ease-linear md:left-[var(--sidebar-width)] md:px-8 group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)] ${showScrollDownButton ? 'chat-scroll-down-enter' : 'chat-scroll-down-exit'}`}
          >
            <div class="mx-auto flex w-full max-w-[760px] justify-end">
              <Button
                type="button"
                variant="secondary"
                size="icon"
                class="pointer-events-auto rounded-none shadow-none"
                aria-label="Scroll to bottom"
                title="Scroll to bottom"
                onclick={scrollChatToBottom}
              >
                <CaretDownIcon class="size-4" />
              </Button>
            </div>
          </div>
        {/if}

        <SessionComposerDock
          bind:height={composerHeight}
          bind:input={$chatDraft}
          session={selectedSession}
          gitStatus={selectedSessionGitStatus}
          metadataItems={selectedSessionMetadataItems}
          metadataSummary={selectedSessionMetadataSummary}
          queuedMessages={visibleInboxMessages}
          inboxBusyMessageId={inboxActionMessageId}
          {submitting}
          {actionBusy}
          {canSend}
          autofocus={autofocusComposer}
          onCancelInboxMessage={(message) => void cancelPendingInboxMessage(message)}
          onRetryInboxMessage={(message) => void retryFailedInboxMessage(message)}
          onDismissInboxMessage={(message) => void dismissFailedInboxMessage(message)}
          onSend={() => void sendMessage()}
          onNewChat={openNewChat}
          onRename={(title) => void renameSelectedSession(title)}
          onExit={() => void exitSelectedSession()}
          onInterrupt={() => void interruptSelectedSession()}
          onFocus={() => void refreshCurrentSessionGitStatus()}
        />
      {/if}
    </div>
  </div>
</section>

{#if selectedSession && !initialChatScrollPending && rulerTurns.length}
  <ChatRuler
    turns={rulerTurns}
    treeMode={rulerTreeMode}
    navigableTurnIds={rulerNavigableTurnIds}
    onNavigate={navigateFromRuler}
  />
{/if}

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
