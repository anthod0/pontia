<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import { clientControlDetails, clientResumeWaitsForIdle } from '../clients'
  import { onDestroy, onMount, tick } from 'svelte'
  import { get } from 'svelte/store'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { navigate } from '$lib/navigation'
  import { dashboardScope } from '$lib/dashboardRoutes'
  import { claimChatEntryAutofocus } from '$lib/chatEntryAutofocus'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import SessionConversationSkeleton from '$lib/components/session-chat/SessionConversationSkeleton.svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import AgentStatus from '$lib/components/session-chat/AgentStatus.svelte'
  import ChatRuler from '$lib/components/session-chat/ChatRuler.svelte'
  import type { HistoryFork } from '../stores/historyForks'
  import { SubmissionUnconfirmedError } from '../stores/inboxRecovery'
  import type { DashboardStreamEvent, EventView, InboxDeliveryPolicy, InboxMessageView, SessionView, TurnView } from '../api/types'
  import { compactionDisabledReason, compactionStatus, compactionEventItems } from '$lib/session-chat/compaction'
  import type { ChatMessageRole, SessionChatMessage } from '$lib/session-chat/sessionChat'
  import {
    canSendSessionMessage,
    timelineItemsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import type { LiveOutputEvent, LiveOutputOverlays } from '$lib/session-chat/liveOutput'
  import {
    applyLiveOutputEvent,
    markLiveOutputDisconnected,
    mergeLiveOutputMessages,
    removeLiveOutputOverlay,
  } from '$lib/session-chat/liveOutput'
  import { hasTimelineSnapshot } from "../stores/timeline";
  import SessionComposerDock from '../components/chat/SessionComposerDock.svelte'
  import GitPanelSession from '../components/chat/git/GitPanelSession.svelte'
  import SessionInteractions from '../components/chat/SessionInteractions.svelte'
  import ScrollToBottomButton from '../components/chat/ScrollToBottomButton.svelte'
  import { scrollDocumentToBottom } from '../lib/session-chat/autoScroll'
  import { sessionMetadataItems, sessionMetadataSummary, visibleChatInboxMessages } from '../components/chat/sessionMetadata'
  import { isExecutingSessionState, isTerminalSession } from '$lib/sessionState'
  const device = useDeviceRuntime();
  const deviceScope = dashboardScope();
  const { pending: pendingHistoryForks } = device.historyForks;
  const { retryInboxMessage } = device.sessions;
  const { compactSession } = device.api;
  const {
    chatMessagesWithOptimistic,
    createOptimisticChatQuery,
    reconcileOptimisticMessages,
  } = device.optimisticChat;
  const {
    createOptimisticInboxQuery,
    inboxSubmissionMessages,
    reconcileInboxSubmissions,
  } = device.inboxQueries;
  const { chatDraft, clearChatDraft } = device.chatDraft;
  const {
    createRefreshWorkspaceGitStatusMutation,
    createWorkspaceGitStatusesStore,
    createWorkspacesStore,
    invalidateWorkspaceGit,
  } = device.workspaceQueries;
  const {
    cancelInboxMessage,
    dismissInboxMessage,
    loadSessionDetail,
    interruptSession,
    terminateSession,
    pinSession,
    unpinSession,
    archiveSession,
    unarchiveSession,
    resumeSession,
    sessionDetail,
    sessionDetailLoading,
    sessionDetailError,
    sessionDetailErrorKind,
    selectSession,
    submitInboxMessage,
    updateSessionTitle,
  } = device.sessions;
  const {
    loadSessionTimeline,
    refreshSessionTimeline,
    resetTimelineState,
    restoreSessionTimeline,
    timelineState,
  } = device.timeline;
  const { subscribeDashboardEvents } = device.events;
  const { openLiveOutputStream } = device.liveOutput;

  const { queuedInboxMessages } = device.inboxQueries;

  export let routeSessionId: string | null = null

  let selectedSessionId = ''
  let composerHeight = 180
  let submitting = false
  let branchActionSubmitting = false
  let branchActionError: string | null = null
  let actionBusy = false
  let inboxActionMessageId: string | null = null
  let actionError: string | null = null
  let compactionEvents: EventView[] = []
  let requestedCompaction: string | null = null
  let unsubscribeDashboardEvents: (() => void) | null = null
  let closeLiveOutputStream: (() => void) | null = null
  let liveOutputGeneration = 0
  let liveOutputOverlays: LiveOutputOverlays = {}
  let autofocusComposer = false
  let bottomSentinel: HTMLElement | null = null
  let scrollDownButton: ScrollToBottomButton | null = null
  interface PromptScrollRequest {
    sessionId: string
    input: string
    turnIds: Set<string>
  }
  let pendingPromptScrolls: PromptScrollRequest[] = []
  let promptScrollScheduled = false
  let historyObserverEnabled = false
  let initialChatScrollPending = false
  let transcriptResolving = false
  let selectedSessionLoadGeneration = 0
  let destroyed = false
  let selectedWorkspaceId = ''
  let gitStatusWorkspaceId = ''
  let gitStatusSessionId = ''

  const workspacesQuery = createWorkspacesStore()
  const workspaceGitStatuses = createWorkspaceGitStatusesStore()
  const refreshWorkspaceGitStatusMutation = createRefreshWorkspaceGitStatusMutation()
  const optimisticChatQuery = createOptimisticChatQuery()
  const optimisticInboxQuery = createOptimisticInboxQuery()

  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const BRANCH_INTERRUPT_TIMEOUT_MS = 30_000
  const INITIAL_SCROLL_SETTLE_PASSES = 2

  onMount(() => {
    selectedSessionId = requestedSessionIdFromLocation()
    selectSession(selectedSessionId || null)
    autofocusComposer = claimChatEntryAutofocus(`${device.id}:/chat/${selectedSessionId}`)
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
    if (selectedSessionId) void loadSelectedSession(selectedSessionId)
    // Some integrations expose control health independently of lifecycle facts.
    let refreshingControl = false
    const controlRefresh = setInterval(async () => {
      if (refreshingControl || (!controlDetails?.refreshWhileActive && selectedSession?.control_connected === undefined) || !selectedSession || isTerminalSession(selectedSession)) return
      refreshingControl = true
      try {
        await loadSessionDetail(selectedSession.session_id, { showLoading: false })
      } finally {
        refreshingControl = false
      }
    }, 2_000)
    return () => clearInterval(controlRefresh)
  })

  onDestroy(() => {
    destroyed = true
    if (dashboardScope() === deviceScope) {
      selectSession(null)
      resetTimelineState()
    }
    unsubscribeDashboardEvents?.()
    closeLiveOutputStream?.()
  })

  $: selectedSession = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.session : null
  $: controlDetails = selectedSession ? clientControlDetails(selectedSession) : null
  $: liveOutputKey = selectedSession ? `${selectedSession.session_id}:${selectedSession.capabilities.stream_output === true}:${$timelineState.status}:${$timelineState.historySource ?? ""}` : ''
  let activeLiveOutputKey = ''
  $: if (!destroyed && liveOutputKey !== activeLiveOutputKey) {
    activeLiveOutputKey = liveOutputKey
    startSelectedLiveOutput(selectedSession)
  }
  $: if ($timelineState.sessionId === selectedSessionId && !$timelineState.loading && $timelineState.status !== 'idle') {
    historyObserverEnabled = true
  }
  $: selectedWorkspaceId = selectedSession?.workspace_id ?? ''
  $: if (selectedWorkspaceId !== gitStatusWorkspaceId) {
    gitStatusWorkspaceId = selectedWorkspaceId
    refreshWorkspaceGitStatusMutation.reset()
  }
  $: if (selectedSession && selectedSession.session_id !== gitStatusSessionId) {
    gitStatusSessionId = selectedSession.session_id
    void refreshSessionGitStatus(selectedSession)
  }
  $: workspaces = $workspacesQuery.data ?? []
  $: selectedSessionGitStatus = $workspaceGitStatuses[selectedWorkspaceId]
  $: workspaceGitStatusErrors = selectedWorkspaceId
    && refreshWorkspaceGitStatusMutation.variables === selectedWorkspaceId
    && refreshWorkspaceGitStatusMutation.error
    ? { [selectedWorkspaceId]: refreshWorkspaceGitStatusMutation.error instanceof Error
      ? refreshWorkspaceGitStatusMutation.error.message
      : String(refreshWorkspaceGitStatusMutation.error) }
    : {}
  $: selectedSessionMetadataItems = selectedSession ? sessionMetadataItems(selectedSession, workspaces, selectedSessionGitStatus, workspaceGitStatusErrors, $t) : []
  $: selectedSessionMetadataSummary = sessionMetadataSummary(selectedSessionMetadataItems, $t)
  $: transcriptMessages = $timelineState.sessionId === selectedSessionId
    ? timelineItemsToChatMessages($timelineState.items, true, $t)
    : []
  $: selectedTurns = $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.turns : []
  $: selectedCompactionEvents = [
    ...($sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.events : []),
    ...compactionEvents,
  ]
  $: selectedCompaction = compactionStatus(selectedCompactionEvents, requestedCompaction, $t)
  $: eventCheckpoints = selectedSession?.capabilities.compaction_events === true && selectedSession?.capabilities.compaction_history !== true
  $: conversationTimelineItems = eventCheckpoints
    ? compactionEventItems($timelineState.sessionId === selectedSessionId ? $timelineState.items : [], selectedCompactionEvents)
    : $timelineState.sessionId === selectedSessionId ? $timelineState.items : []
  $: compactedHistoryNotice = eventCheckpoints && selectedCompactionEvents.some(event => event.type === "session.compaction_completed")
  $: timelineMessages = mergeLiveOutputMessages(
    transcriptMessages,
    selectedTurns,
    liveOutputOverlays,
    selectedSession?.current_turn_id ?? null,
    !sessionSupportsTimeline(selectedSession),
  )
  $: reconcileOptimisticMessages(selectedSessionId, timelineMessages)
  $: reconcileInboxSubmissions(selectedSessionId, timelineMessages)
  $: messages = inboxSubmissionMessages(
    chatMessagesWithOptimistic(timelineMessages, $optimisticChatQuery.data?.[selectedSessionId] ?? []),
    $optimisticInboxQuery.data?.[selectedSessionId] ?? [],
  )
  $: branchActionInputs = eligibleBranchActionInputs(selectedSession, messages)
  $: branchActionMessageIds = Object.keys(branchActionInputs)
  $: timelineUnavailable = $timelineState.sessionId === selectedSessionId && Boolean($timelineState.error)
    && !($timelineState.status === 'pending' && $timelineState.items.length > 0)
  $: rulerTurns = $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.turns : []
  $: rulerTreeMode = $timelineState.sessionId === selectedSessionId && $timelineState.mode === 'tree'
  $: rulerNavigableTurnIds = navigableRulerTurnIds(
    rulerTurns,
    rulerTreeMode,
    $timelineState.sessionId === selectedSessionId ? $timelineState.latestTurnId : null,
  )
  $: selectedInboxMessages = selectedSessionId && $sessionDetail?.session.session_id === selectedSessionId ? $sessionDetail.inboxMessages : []
  $: selectedInboxSubmissions = $optimisticInboxQuery.data?.[selectedSessionId] ?? []
  $: visibleInboxMessages = queuedInboxMessages(visibleChatInboxMessages(selectedInboxMessages), selectedInboxSubmissions)
  $: pendingInboxSubmissionId = selectedInboxSubmissions.find((submission) => !submission.acceptedMessage)?.localId ?? null
  $: canSend = canSendSessionMessage(selectedSession, 'x') && !submitting
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
    if (!session?.capabilities.branch_control && !session?.capabilities.fork_session) return {}
    const canEditInCurrentState = ['idle', 'interrupted', 'exited'].includes(session.state)
      || (isExecutingSessionState(session.state) && session.capabilities.interrupt === true)
    if (!session.capabilities.fork_session && !canEditInCurrentState) return {}
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
      if (session.capabilities.fork_session && (projectedTurn.state === 'abandoned' || !projectedTurn.metadata.native_turn_id)) return []
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
    return detail?.session.session_id === selectedSessionId ? detail.session : null
  }

  async function refreshSessionGitStatus(session: SessionView | null): Promise<void> {
    const workspaceId = session?.workspace_id
    if (!workspaceId) return
    try {
      await refreshWorkspaceGitStatusMutation.mutateAsync(workspaceId)
    } catch {
      // The mutation exposes the error to the metadata UI.
    }
  }

  /** An Agent turn may have changed files; the Git panel reads them again if it is open. */
  function refreshGitAfterTurn(): void {
    const workspaceId = currentSelectedSession()?.workspace_id
    if (workspaceId) void invalidateWorkspaceGit(workspaceId)
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
    if (!selectedSessionId || !['failed', 'unknown'].includes(message.state)) return
    if (message.state === 'unknown' && !window.confirm($t('chat.duplicateConfirm'))) return
    inboxActionMessageId = message.message_id
    actionError = null
    try {
      await retryInboxMessage(selectedSessionId, message, message.state === 'unknown')
    } catch (error) {
      actionError = error instanceof SubmissionUnconfirmedError ? null : error instanceof Error ? error.message : String(error)
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
      if (session?.capabilities.fork_session) {
        const existing = $pendingHistoryForks.find((operation) => operation.sourceSessionId === sessionId && operation.turnId === projectedTurn.turn_id)
        if (existing) {
          branchActionError = $t('chat.recoverBeforeEdit')
          return false
        }
        const operation = await device.historyForks.start(sessionId, projectedTurn.turn_id, input)
        if (operation.sessionId && ['session_created', 'input_unconfirmed', 'input_submitted'].includes(operation.stage)) {
          await navigate(`/chat/${operation.sessionId}`, undefined, { scope: deviceScope })
          return true
        }
        return false
      }
      if (session && isExecutingSessionState(session.state)) {
        if (session.capabilities.interrupt !== true) {
          throw new Error($t('chat.cannotInterrupt'))
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
      return error instanceof SubmissionUnconfirmedError
    } finally {
      branchActionSubmitting = false
    }
  }

  async function recoverHistoryFork(operation: HistoryFork): Promise<void> {
    if (branchActionSubmitting) return
    branchActionSubmitting = true
    try {
      const result = await device.historyForks.resume(operation)
      if (result.sessionId && ['session_created', 'input_unconfirmed', 'input_submitted'].includes(result.stage)) {
        await navigate(`/chat/${result.sessionId}`, undefined, { scope: deviceScope })
      }
    } catch (error) {
      branchActionError = error instanceof Error ? error.message : String(error)
    } finally {
      branchActionSubmitting = false
    }
  }

  function editHistoricalMessage(message: SessionChatMessage, replacementInput: string): Promise<boolean> {
    return submitBranchEdit(message, replacementInput)
  }

  function nextAnimationFrame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()))
  }

  function scrollChatToBottom(): void {
    scrollDocumentToBottom()
    scrollDownButton?.hide()
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
    if (get(timelineState).historySource === 'file') return
    liveOutputOverlays = applyLiveOutputEvent(liveOutputOverlays, selectedSessionId, event)
  }

  function startSelectedLiveOutput(session: SessionView | null): void {
    const generation = ++liveOutputGeneration
    closeLiveOutputStream?.()
    closeLiveOutputStream = null
    const timeline = get(timelineState)
    if (session?.capabilities.timeline && (timeline.status === 'loading' || timeline.status === 'idle' || timeline.historySource === 'file')) {
      liveOutputOverlays = {}
      return
    }
    if (!session || session.session_id !== selectedSessionId || session.capabilities.stream_output !== true) return
    closeLiveOutputStream = openLiveOutputStream(session.session_id, {
      onEvent: (event) => { if (generation === liveOutputGeneration) handleLiveOutputEvent(event) },
      onDisconnected: () => {
        if (generation === liveOutputGeneration) liveOutputOverlays = markLiveOutputDisconnected(liveOutputOverlays)
      },
    })
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === 'history_changed' && streamEvent.session_id === selectedSessionId) {
      ++liveOutputGeneration
      ++selectedSessionLoadGeneration
      closeLiveOutputStream?.()
      closeLiveOutputStream = null
      liveOutputOverlays = {}
      resetTimelineState(selectedSessionId)
      void loadSessionTimeline(selectedSessionId, { mode: 'rebuild', topology: selectedSession?.capabilities.topology === true })
      return
    }
    if (streamEvent.kind === 'session_event') {
      if (streamEvent.event.session_id !== selectedSessionId) return
      if (isTerminalTurnEvent(streamEvent.event.type)) refreshGitAfterTurn()
      if (streamEvent.event.type.startsWith('session.compaction_')) {
        compactionEvents = [...compactionEvents.filter(event => event.event_id !== streamEvent.event.event_id), streamEvent.event]
        void loadSessionDetail(selectedSessionId, { showLoading: false })
        if (streamEvent.event.type === 'session.compaction_completed' && sessionSupportsTimeline(currentSelectedSession())) {
          if (currentSelectedSession()?.capabilities.compaction_history) void refreshSessionTimeline(selectedSessionId)
          else void loadSessionTimeline(selectedSessionId, { mode: 'rebuild', topology: false })
        }
        return
      }
      if (streamEvent.event.type === 'session.model_updated') {
        void loadSessionDetail(selectedSessionId, { showLoading: false })
        return
      }
      if (!sessionSupportsTimeline(currentSelectedSession())) {
        void loadSessionDetail(selectedSessionId, { showLoading: false })
        return
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
    if (dashboardScope() !== deviceScope) return
    const nextSessionId = requestedSessionIdFromLocation()
    if (nextSessionId === selectedSessionId) return
    closeLiveOutputStream?.()
    closeLiveOutputStream = null
    liveOutputOverlays = {}
    compactionEvents = []
    requestedCompaction = null
    selectedSessionId = nextSessionId
    gitStatusSessionId = ''
    selectSession(selectedSessionId || null)
    resetTimelineState(selectedSessionId)
    pendingPromptScrolls = []
    actionError = null
    branchActionError = null
    if (selectedSessionId) {
      autofocusComposer = claimChatEntryAutofocus(`${device.id}:/chat/${selectedSessionId}`)
      await loadSelectedSession(selectedSessionId)
    } else {
      historyObserverEnabled = false
      initialChatScrollPending = false
      resetTimelineState()
    }
  }

  function sessionSupportsTimeline(session: SessionView | null): boolean {
    return session?.capabilities.timeline === true || session?.capabilities.compaction_history === true
  }

  function timelineMode(topology: boolean): 'linear' | 'tree' {
    return topology ? 'tree' : 'linear'
  }

  function timelineMatches(sessionId: string, topology: boolean | null): boolean {
    const state = get(timelineState)
    if (!hasTimelineSnapshot(state, sessionId) || state.mode === null) return false
    return topology === null || state.mode === timelineMode(topology)
  }

  function sessionLoadIsCurrent(sessionId: string, generation: number): boolean {
    return !destroyed && selectedSessionId === sessionId && selectedSessionLoadGeneration === generation
  }

  async function rebuildTimeline(sessionId: string, generation: number, topology: boolean): Promise<void> {
    if (!sessionLoadIsCurrent(sessionId, generation)) return
    initialChatScrollPending = true
    resetTimelineState(sessionId)
    await loadSessionTimeline(sessionId, {
      mode: 'rebuild',
      latestTurnId: latestProjectedTurnId(),
      ...(topology ? { topology: true } : {}),
    })
    if (!sessionLoadIsCurrent(sessionId, generation)) return
    await scrollChatToBottomAfterLayout()
    if (!sessionLoadIsCurrent(sessionId, generation)) return
    initialChatScrollPending = false
    historyObserverEnabled = true
  }

  async function revealCachedTimeline(
    sessionId: string,
    generation: number,
    detailPromise: Promise<unknown>,
  ): Promise<void> {
    const cached = get(timelineState)
    initialChatScrollPending = false
    transcriptResolving = false
    // The conversation renders only once the Session detail is selected.
    const conversationRendered = currentSelectedSession() ? Promise.resolve() : detailPromise
    void conversationRendered.then(() => {
      if (sessionLoadIsCurrent(sessionId, generation)) return scrollChatToBottomAfterLayout()
    })
    void refreshSessionTimeline(sessionId, cached.latestTurnId)
    await detailPromise
    if (!sessionLoadIsCurrent(sessionId, generation)) return
    const loadedSession = currentSelectedSession()
    if (!loadedSession || !sessionSupportsTimeline(loadedSession)) {
      resetTimelineState(sessionId)
      return
    }
    const expectedMode = timelineMode(loadedSession.capabilities.topology === true)
    if (get(timelineState).sessionId === sessionId && get(timelineState).mode === expectedMode) {
      historyObserverEnabled = true
      return
    }
    await rebuildTimeline(sessionId, generation, expectedMode === 'tree')
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    const generation = ++selectedSessionLoadGeneration
    historyObserverEnabled = false
    const knownSession = currentSelectedSession()
    if (knownSession && !sessionSupportsTimeline(knownSession)) {
      initialChatScrollPending = false
      transcriptResolving = false
      resetTimelineState(sessionId)
      await loadSessionDetail(sessionId)
      return
    }

    const knownTopology = knownSession ? knownSession.capabilities.topology === true : null
    const detailPromise = loadSessionDetail(sessionId)
    try {
      let hasLoadedTimeline = timelineMatches(sessionId, knownTopology)
      if (!hasLoadedTimeline) {
        transcriptResolving = true
        initialChatScrollPending = false
        const current = get(timelineState)
        if (current.sessionId && current.sessionId !== sessionId) resetTimelineState(sessionId)
        const restored = await restoreSessionTimeline(
          sessionId,
          knownTopology === null ? {} : { topology: knownTopology },
        )
        if (!sessionLoadIsCurrent(sessionId, generation)) {
          void detailPromise.catch(() => undefined)
          return
        }
        transcriptResolving = false
        hasLoadedTimeline = restored && timelineMatches(sessionId, knownTopology)
      }

      if (hasLoadedTimeline) {
        await revealCachedTimeline(sessionId, generation, detailPromise)
        return
      }

      initialChatScrollPending = true
      await detailPromise
      if (!sessionLoadIsCurrent(sessionId, generation)) return
      const loadedSession = currentSelectedSession()
      if (!loadedSession || !sessionSupportsTimeline(loadedSession)) {
        initialChatScrollPending = false
        resetTimelineState(sessionId)
        return
      }
      await loadSessionTimeline(sessionId, {
        mode: 'rebuild',
        latestTurnId: latestProjectedTurnId(),
        ...(loadedSession.capabilities.topology === true ? { topology: true } : {}),
      })
      if (!sessionLoadIsCurrent(sessionId, generation)) return
      await scrollChatToBottomAfterLayout()
      if (!sessionLoadIsCurrent(sessionId, generation)) return
      initialChatScrollPending = false
      historyObserverEnabled = true
    } catch (error) {
      if (sessionLoadIsCurrent(sessionId, generation)) {
        transcriptResolving = false
        initialChatScrollPending = false
      } else {
        void detailPromise.catch(() => undefined)
      }
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

  async function compactSelectedSession(): Promise<void> {
    if (!selectedSession || actionBusy || compactionDisabledReason(selectedSession, selectedCompaction?.busy ?? false, $t)) return
    const sessionId = selectedSessionId
    actionBusy = true
    actionError = null
    try {
      const receipt = await compactSession(sessionId)
      if (selectedSessionId !== sessionId) return
      requestedCompaction = receipt.operation_id
      if ($chatDraft.trim() === '/compact') clearChatDraft()
      await loadSessionDetail(sessionId, { showLoading: false })
      if (selectedSessionId === sessionId && $sessionDetail?.events.some(event => event.type === 'session.compaction_completed' && event.payload.operation_id === receipt.operation_id)) {
        await refreshSessionTimeline(sessionId)
      }
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

  async function toggleSelectedSessionManagement(command: '/pin' | '/archive'): Promise<void> {
    if (!selectedSession || actionBusy) return
    const session = selectedSession
    const sessionId = session.session_id
    actionBusy = true
    actionError = null
    try {
      if (command === '/pin') {
        if (session.archived_at) return
        await (session.pinned_at ? unpinSession : pinSession)(sessionId)
      } else {
        await (session.archived_at ? unarchiveSession : archiveSession)(sessionId)
      }
      if (selectedSessionId === sessionId && $chatDraft.trim() === command) clearChatDraft()
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
      if (sessionSupportsTimeline(currentSelectedSession())) {
        await refreshSessionTimeline(selectedSessionId, selectedSession?.current_turn_id ?? latestProjectedTurnId())
      }
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  function sessionStateFromDetail(sessionId: string): string | null {
    const detail = get(sessionDetail)
    return detail?.session.session_id === sessionId ? detail.session.state : null
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
        finish(() => reject(new Error($t('chat.interruptTimeout'))))
      }, timeoutMs)

      const stop = sessionDetail.subscribe(check)
      unsubscribe = stop
      if (done) stop()
      else check()
    })
  }

  function waitForSessionIdle(sessionId: string, timeoutMs = AUTO_RESUME_IDLE_TIMEOUT_MS): Promise<void> {
    if (sessionStateFromDetail(sessionId) === 'idle') return Promise.resolve()

    return new Promise((resolve, reject) => {
      let done = false
      let unsubscribeDetail: (() => void) | null = null

      const cleanup = () => {
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
        if (sessionStateFromDetail(sessionId) === 'idle') finish(resolve)
      }
      const timeout = setTimeout(() => {
        finish(() => reject(new Error($t('chat.resumeTimeout'))))
      }, timeoutMs)

      unsubscribeDetail = sessionDetail.subscribe(check)
      check()
    })
  }

  async function sendMessage(message: string, deliveryPolicy: InboxDeliveryPolicy): Promise<boolean> {
    if (!canSend || !selectedSessionId || !message.trim()) return false
    submitting = true
    actionError = null
    const originalDraft = $chatDraft
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
        if (clientResumeWaitsForIdle(selectedSession)) await waitForSessionIdle(selectedSessionId)
        clearChatDraft()
      }
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: deliveryPolicy,
        metadata: { source: 'dashboard_chat' },
      })
      return true
    } catch (error) {
      pendingPromptScrolls = pendingPromptScrolls.filter((request) => request !== scrollRequest)
      if (!get(chatDraft).trim()) chatDraft.set(originalDraft)
      actionError = error instanceof SubmissionUnconfirmedError ? null : error instanceof Error ? error.message : String(error)
      return false
    } finally {
      submitting = false
    }
  }
</script>

<svelte:window onpopstate={() => void selectSessionFromLocation()} />

{#if selectedSession?.workspace_id}
  <GitPanelSession workspaceId={selectedSession.workspace_id} gitStatus={selectedSessionGitStatus} />
{/if}

<section class="flex flex-col gap-4 pb-[var(--chat-bottom-padding)]" style={`--chat-top-offset: 4rem; --chat-bottom-padding: ${composerHeight + 16}px; --chat-composer-height: ${composerHeight}px`}>
  {#if selectedSession?.control_connected === false}
    <p class="text-sm text-destructive" role="status">{$t(selectedSession.control_disconnected_reason === 'unsupported_protocol_version' ? 'chat.controlProtocolMismatch' : 'chat.clientDisconnected')}</p>
  {/if}
  {#if selectedSession && controlDetails && (selectedSession.capabilities.accept_task || controlDetails.unavailableReason) && (controlDetails.unavailableReason || controlDetails.profile?.error || !sessionSupportsTimeline(selectedSession))}
    <div class="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
      {#if controlDetails.unavailableReason}
        <span class="text-destructive" role="alert">{controlDetails.unavailableReason}</span>
      {/if}
      {#if controlDetails.profile?.error}
        <span class="text-destructive" role="alert">{controlDetails.profile.error}</span>
      {/if}
      {#if !sessionSupportsTimeline(selectedSession)}<span>{$t('chat.historyUnavailable')}</span>{/if}
    </div>
  {/if}
  {#if $sessionDetailError && selectedSession}
    <Alert.Root variant="destructive" role="alert" class="mx-auto w-full max-w-[760px]">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('chat.refreshFailed')}</Alert.Title>
      <Alert.Description>{$sessionDetailError} {$t('chat.lastSnapshot')}</Alert.Description>
    </Alert.Root>
  {/if}
  {#if actionError}
    <Alert.Root variant="destructive" role="alert" class="mx-auto w-full max-w-[760px]">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('chat.actionFailed')}</Alert.Title>
      <Alert.Description>{actionError}</Alert.Description>
    </Alert.Root>
  {/if}
  <div class="mx-auto min-w-0 w-full max-w-[760px] flex-1">
    <div class="relative flex min-w-0 flex-col rounded-none bg-transparent">
      {#if $sessionDetailLoading && !selectedSession}
        <SessionConversationSkeleton class="min-h-80" />
      {:else if !selectedSession && $sessionDetailErrorKind !== 'not_found'}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Title>{$sessionDetailErrorKind === 'authentication' ? $t('auth.required') : $sessionDetailErrorKind === 'network' ? $t('connection.failed') : $t('chat.loadFailed')}</Empty.Title>
            <Empty.Description>{$sessionDetailError ?? $t('chat.loading')} {$t('chat.reloadOnReconnect')}</Empty.Description>
          </Empty.Header>
          <Empty.Content><Button onclick={() => void loadSelectedSession(selectedSessionId)} disabled={$sessionDetailLoading}>{$t('common.retry')}</Button></Empty.Content>
        </Empty.Root>
      {:else if !selectedSession}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Title>{$t('chat.notFound')}</Empty.Title>
            <Empty.Description>{$t('chat.chooseSession')}</Empty.Description>
          </Empty.Header>
          <Empty.Content><Button onclick={() => openNewChat()}>{$t('chat.startNew')}</Button></Empty.Content>
        </Empty.Root>
      {:else}
        <div
          data-chat-initial-scroll-pending={initialChatScrollPending || transcriptResolving ? 'true' : 'false'}
          class={initialChatScrollPending || transcriptResolving ? 'relative min-h-80' : 'relative'}
        >
          {#if initialChatScrollPending}
            <SessionConversationSkeleton class="pointer-events-none absolute inset-x-0 top-0 z-10" />
          {/if}
          <div class={initialChatScrollPending || transcriptResolving ? 'opacity-0' : ''}>
            {#if !sessionSupportsTimeline(selectedSession) || timelineUnavailable}
              {#if selectedSession.state === 'waiting_for_interaction'}
                <AgentStatus state={selectedSession.state} />
              {/if}
              <Empty.Root data-timeline-status={$timelineState.status} class="min-h-80">
                <Empty.Header>
                  <Empty.Title>{$timelineState.status === 'pending' ? $t('chat.waitingHistory') : $t('chat.conversationUnavailable')}</Empty.Title>
                  <Empty.Description>{!sessionSupportsTimeline(selectedSession) ? selectedSession.timeline_unavailable_reason ?? $t('chat.noConversationHistory') : $timelineState.error}</Empty.Description>
                </Empty.Header>
                <Empty.Content>
                  <Button variant="outline" onclick={() => loadSelectedSession(selectedSessionId)}>{$t('chat.retryHistory')}</Button>
                </Empty.Content>
              </Empty.Root>
            {:else}
              {#if compactedHistoryNotice}
                <p role="status" class="py-2 text-sm text-muted-foreground">{$t('compaction.earlierHistory')}</p>
              {/if}
              {#if $timelineState.status === 'pending'}
                <p role="status" class="py-2 text-sm text-muted-foreground">{$t('chat.loadingHistory')}</p>
              {/if}
              {#if $timelineState.compactionHistoryError}
                <Alert.Root class="my-3" role="status">
                  <WarningCircleIcon class="size-4" />
                  <Alert.Title>{$t('chat.compactionHistoryUnavailable')}</Alert.Title>
                  <Alert.Description>
                    {$timelineState.compactionHistoryError}
                    <Button variant="outline" size="sm" disabled={$timelineState.refreshing} onclick={() => void refreshSessionTimeline(selectedSessionId)}>{$t('common.retry')}</Button>
                  </Alert.Description>
                </Alert.Root>
              {/if}
              {#each $timelineState.groups.filter((group) => group.history_issue) as group (group.turn_id)}
                <Alert.Root class="my-3" role="status">
                  <WarningCircleIcon class="size-4" />
                  <Alert.Title>{$t('chat.historyUnassociated')}</Alert.Title>
                  <Alert.Description>
                    {#if group.history_issue === 'topology_unknown'}
                      {$t('chat.branchUnconfirmed')}
                    {:else}
                      {$t('chat.unconfirmedRange')}
                    {/if}
                  </Alert.Description>
                </Alert.Root>
              {/each}
            {/if}
            {#if (sessionSupportsTimeline(selectedSession) && !timelineUnavailable) || (!sessionSupportsTimeline(selectedSession) && (selectedSession.capabilities.stream_output || Object.keys(liveOutputOverlays).length > 0))}
              {#key selectedSessionId}
                <SessionConversation
                  {messages}
                  timelineItems={conversationTimelineItems}
                  compactionBusy={selectedCompaction?.busy ?? false}
                  sessionId={selectedSessionId}
                  workspace={selectedSession.workspace}
                  turns={selectedTurns}
                  sessionState={selectedSession.state}
                  activeTurnId={selectedSession.current_turn_id}
                  loading={(initialChatScrollPending || transcriptResolving || $sessionDetailLoading || $timelineState.loading) && !messages.length}
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
        <SessionInteractions sessionId={selectedSessionId} />
        {#each $pendingHistoryForks.filter((operation) => operation.sourceSessionId === selectedSessionId || operation.sessionId === selectedSessionId) as operation (operation.operationId)}
          <Alert.Root variant="destructive" class="mx-4 my-2">
            <WarningCircleIcon class="size-4" />
            <Alert.Title>{operation.stage === 'fork_failed' ? $t('chat.forkFailed') : operation.stage === 'session_pending' ? $t('chat.forkPending') : operation.stage === 'input_unconfirmed' ? $t('chat.submissionUnconfirmed') : $t('chat.forkUnconfirmed')}</Alert.Title>
            <Alert.Description>
              {operation.error ?? $t('chat.recoverProgress')}
              <div class="mt-2 flex gap-2">
                {#if operation.stage === 'fork_failed'}
                  <Button variant="outline" size="sm" onclick={() => device.historyForks.forget(operation.operationId)}>{$t('common.dismiss')}</Button>
                {:else}
                  <Button variant="outline" size="sm" disabled={branchActionSubmitting} onclick={() => recoverHistoryFork(operation)}>{$t('chat.recoverEdit')}</Button>
                {/if}
              </div>
            </Alert.Description>
          </Alert.Root>
        {/each}

        {#if branchActionError}
          <Alert.Root variant="destructive" role="alert" class="mx-4 mb-4">
            <WarningCircleIcon class="size-4" />
            <Alert.Title>{$t('chat.branchFailed')}</Alert.Title>
            <Alert.Description>{branchActionError}</Alert.Description>
          </Alert.Root>
        {/if}
        <div aria-hidden="true" class="absolute bottom-0 h-px w-px" data-chat-bottom-sentinel bind:this={bottomSentinel}></div>
        <ScrollToBottomButton
          bind:this={scrollDownButton}
          sentinel={bottomSentinel}
          onclick={scrollDocumentToBottom}
          class="fixed bottom-[calc(var(--chat-composer-height)+0.5rem)] left-0 right-0 lg:right-[var(--file-preview-width,0px)] z-40 px-4 transition-[left] duration-200 ease-linear md:left-[var(--sidebar-width)] md:px-8 group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]"
        />

        <SessionComposerDock
          bind:height={composerHeight}
          bind:input={$chatDraft}
          session={selectedSession}
          gitStatus={selectedSessionGitStatus}
          metadataItems={selectedSessionMetadataItems}
          metadataSummary={selectedSessionMetadataSummary}
          queuedMessages={visibleInboxMessages}
          inboxBusyMessageId={inboxActionMessageId ?? pendingInboxSubmissionId}
          {submitting}
          {actionBusy}
          compaction={selectedCompaction}
          {canSend}
          autofocus={autofocusComposer}
          onCancelInboxMessage={(message) => void cancelPendingInboxMessage(message)}
          onRetryInboxMessage={(message) => void retryFailedInboxMessage(message)}
          onDismissInboxMessage={(message) => void dismissFailedInboxMessage(message)}
          onSend={sendMessage}
          onNewChat={openNewChat}
          onRename={(title) => void renameSelectedSession(title)}
          onExit={() => void exitSelectedSession()}
          onCompact={() => void compactSelectedSession()}
          onTogglePin={() => void toggleSelectedSessionManagement('/pin')}
          onToggleArchive={() => void toggleSelectedSessionManagement('/archive')}
          onInterrupt={() => void interruptSelectedSession()}
          onFocus={() => void refreshCurrentSessionGitStatus()}
          onRefreshGitStatus={() => void refreshCurrentSessionGitStatus()}
        />
      {/if}
    </div>
  </div>
</section>

{#if selectedSession && !initialChatScrollPending && !transcriptResolving && rulerTurns.length}
  <ChatRuler
    turns={rulerTurns}
    treeMode={rulerTreeMode}
    navigableTurnIds={rulerNavigableTurnIds}
    onNavigate={navigateFromRuler}
  />
{/if}
