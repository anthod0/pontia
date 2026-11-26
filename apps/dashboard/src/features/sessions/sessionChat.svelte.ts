import type { ChatCommand } from "$lib/chatCommands";
import { isMobileShell } from "$lib/hooks/shellMode.svelte";
import { t } from "$lib/i18n";
import { useDeviceRuntime } from "../../device/context";
import { clientControlDetails, clientResumeWaitsForIdle } from "../../clients";
import { onDestroy, onMount, tick } from "svelte";
import { get } from "svelte/store";
import { navigate } from "$lib/navigation";
import { dashboardScope } from "$lib/dashboardRoutes";
import { claimChatEntryAutofocus } from "$lib/chatEntryAutofocus";
import type { HistoryFork } from "../../stores/historyForks";
import { SubmissionUnconfirmedError } from "../../stores/inboxRecovery";
import type {
  DashboardStreamEvent,
  EventView,
  InboxDeliveryPolicy,
  InboxMessageView,
  SessionView,
  TurnView,
} from "../../api/types";
import {
  compactionDisabledReason,
  compactionStatus,
  compactionEventItems,
} from "$lib/session-chat/compaction";
import type { ChatMessageRole, SessionChatMessage } from "$lib/session-chat/sessionChat";
import { canSendSessionMessage, timelineItemsToChatMessages } from "$lib/session-chat/sessionChat";
import type { LiveOutputEvent, LiveOutputOverlays } from "$lib/session-chat/liveOutput";
import {
  applyLiveOutputEvent,
  markLiveOutputDisconnected,
  mergeLiveOutputMessages,
  removeLiveOutputOverlay,
} from "$lib/session-chat/liveOutput";
import { hasTimelineSnapshot } from "../../stores/timeline";
import { scrollDocumentToBottom } from "../../lib/session-chat/autoScroll";
import {
  sessionMetadataItems,
  sessionMetadataSummary,
  visibleChatInboxMessages,
} from "../../components/chat/sessionMetadata";
import { isExecutingSessionState, isTerminalSession } from "$lib/sessionState";
import { fromStore } from "svelte/store";
import { untrack } from "svelte";
import { createQuery } from "@tanstack/svelte-query";
import { creationOptionCatalog } from "../../queries/modelCatalog";
import { sessionCreationOption } from "../../components/chat/modelEffort";
export function createSessionChat(routeSessionId: () => string | null) {
  const device = useDeviceRuntime();
  const deviceScope = dashboardScope();
  const { pending: pendingHistoryForks } = device.historyForks;
  const { retryInboxMessage } = device.sessions;
  const { compactSession, setSessionReasoningEffort } = device.api;
  const { chatMessagesWithOptimistic, createOptimisticChatQuery, reconcileOptimisticMessages } =
    device.optimisticChat;
  const { createOptimisticInboxQuery, inboxSubmissionMessages, reconcileInboxSubmissions } =
    device.inboxQueries;
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

  let selectedSessionId = $state.raw("");
  let submitting = $state.raw(false);
  let branchActionSubmitting = $state.raw(false);
  let branchActionError = $state.raw<string | null>(null);
  let actionBusy = $state.raw(false);
  let inboxActionMessageId = $state.raw<string | null>(null);
  let actionError = $state.raw<string | null>(null);
  let reasoningEffortBusy = $state.raw(false);
  let reasoningEffortSessionId = "";
  let compactionEvents = $state.raw<EventView[]>([]);
  let requestedCompaction = $state.raw<string | null>(null);
  let unsubscribeDashboardEvents: (() => void) | null = null;
  let closeLiveOutputStream: (() => void) | null = null;
  let liveOutputGeneration = 0;
  let liveOutputOverlays = $state.raw<LiveOutputOverlays>({});
  let autofocusComposer = $state.raw(false);
  let hideScrollButton: (() => void) | undefined;
  interface PromptScrollRequest {
    sessionId: string;
    input: string;
    turnIds: Set<string>;
  }
  let pendingPromptScrolls = $state.raw<PromptScrollRequest[]>([]);
  let promptScrollScheduled = false;
  let historyObserverEnabled = $state.raw(false);
  let initialChatScrollPending = $state.raw(false);
  let transcriptResolving = $state.raw(false);
  let selectedSessionLoadGeneration = 0;
  let destroyed = false;
  let gitStatusWorkspaceId = "";
  let gitStatusSessionId = "";

  const workspacesQuery = createWorkspacesStore();
  const workspaceGitStatuses = createWorkspaceGitStatusesStore();
  const refreshWorkspaceGitStatusMutation = createRefreshWorkspaceGitStatusMutation();
  const optimisticChatQuery = createOptimisticChatQuery();
  const optimisticInboxQuery = createOptimisticInboxQuery();

  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000;
  const BRANCH_INTERRUPT_TIMEOUT_MS = 30_000;
  const INITIAL_SCROLL_SETTLE_PASSES = 2;

  const chatDraftValue = fromStore(chatDraft);
  const optimisticChatQueryValue = fromStore(optimisticChatQuery);
  const optimisticInboxQueryValue = fromStore(optimisticInboxQuery);
  const pendingHistoryForksValue = fromStore(pendingHistoryForks);
  const sessionDetailValue = fromStore(sessionDetail);
  const tValue = fromStore(t);
  const timelineStateValue = fromStore(timelineState);
  const workspaceGitStatusesValue = fromStore(workspaceGitStatuses);
  const workspacesQueryValue = fromStore(workspacesQuery);
  onMount(() => {
    selectedSessionId = requestedSessionIdFromLocation();
    selectSession(selectedSessionId || null);
    autofocusComposer = claimChatEntryAutofocus(`${device.id}:/chat/${selectedSessionId}`);
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent);
    if (selectedSessionId) void loadSelectedSession(selectedSessionId);
    // Some integrations expose control health independently of lifecycle facts.
    let refreshingControl = false;
    const controlRefresh = setInterval(async () => {
      if (
        refreshingControl ||
        (!controlDetails?.refreshWhileActive && selectedSession?.control_connected === undefined) ||
        !selectedSession ||
        isTerminalSession(selectedSession)
      )
        return;
      refreshingControl = true;
      try {
        await loadSessionDetail(selectedSession.session_id, { showLoading: false });
      } finally {
        refreshingControl = false;
      }
    }, 2_000);
    window.addEventListener("popstate", selectSessionFromLocation);
    return () => {
      clearInterval(controlRefresh);
      window.removeEventListener("popstate", selectSessionFromLocation);
    };
  });

  onDestroy(() => {
    destroyed = true;
    if (dashboardScope() === deviceScope) {
      selectSession(null);
      resetTimelineState();
    }
    unsubscribeDashboardEvents?.();
    closeLiveOutputStream?.();
  });

  const selectedSession = $derived(
    selectedSessionId && sessionDetailValue.current?.session.session_id === selectedSessionId
      ? sessionDetailValue.current.session
      : null,
  );

  const creationOptionsQuery = createQuery(
    () => ({
      queryKey: [device.id, "session-creation-options"],
      queryFn: ({ signal }) => device.api.listSessionCreationOptions({ signal }),
      enabled: selectedSession?.capabilities.reasoning_effort_control === true,
    }),
    () => device.queryClient,
  );
  const selectedCreationOption = $derived(
    selectedSession?.capabilities.reasoning_effort_control === true
      ? sessionCreationOption(selectedSession, creationOptionsQuery.data ?? [])
      : undefined,
  );
  const reasoningEffortCatalog = createQuery(
    () =>
      creationOptionCatalog(device, selectedCreationOption, selectedSession?.workspace_id ?? ""),
    () => device.queryClient,
  );
  const reasoningEfforts = $derived(
    reasoningEffortCatalog.data?.models.find((model) => model.id === selectedSession?.model)
      ?.reasoning_efforts ?? [],
  );

  const controlDetails = $derived(selectedSession ? clientControlDetails(selectedSession) : null);

  const liveOutputKey = $derived(
    selectedSession
      ? `${selectedSession.session_id}:${selectedSession.capabilities.stream_output === true}:${timelineStateValue.current.status}:${timelineStateValue.current.historySource ?? ""}`
      : "",
  );

  let activeLiveOutputKey = "";
  $effect(() => {
    if (!destroyed && liveOutputKey !== activeLiveOutputKey)
      untrack(() => {
        activeLiveOutputKey = liveOutputKey;
        startSelectedLiveOutput(selectedSession);
      });
  });
  $effect(() => {
    if (
      timelineStateValue.current.sessionId === selectedSessionId &&
      !timelineStateValue.current.loading &&
      timelineStateValue.current.status !== "idle"
    )
      untrack(() => {
        historyObserverEnabled = true;
      });
  });
  const selectedWorkspaceId = $derived(selectedSession?.workspace_id ?? "");

  $effect(() => {
    if (selectedWorkspaceId !== gitStatusWorkspaceId)
      untrack(() => {
        gitStatusWorkspaceId = selectedWorkspaceId;
        refreshWorkspaceGitStatusMutation.reset();
      });
  });
  $effect(() => {
    if (selectedSession && selectedSession.session_id !== gitStatusSessionId)
      untrack(() => {
        gitStatusSessionId = selectedSession.session_id;
        void refreshSessionGitStatus(selectedSession);
      });
  });
  const workspaces = $derived(workspacesQueryValue.current.data ?? []);

  const selectedSessionGitStatus = $derived(workspaceGitStatusesValue.current[selectedWorkspaceId]);

  const workspaceGitStatusErrors = $derived(
    selectedWorkspaceId &&
      refreshWorkspaceGitStatusMutation.variables === selectedWorkspaceId &&
      refreshWorkspaceGitStatusMutation.error
      ? {
          [selectedWorkspaceId]:
            refreshWorkspaceGitStatusMutation.error instanceof Error
              ? refreshWorkspaceGitStatusMutation.error.message
              : String(refreshWorkspaceGitStatusMutation.error),
        }
      : {},
  );

  const selectedSessionMetadataItems = $derived(
    selectedSession
      ? sessionMetadataItems(
          selectedSession,
          workspaces,
          selectedSessionGitStatus,
          workspaceGitStatusErrors,
          tValue.current,
        )
      : [],
  );

  const selectedSessionMetadataSummary = $derived(
    sessionMetadataSummary(selectedSessionMetadataItems, tValue.current),
  );

  const transcriptMessages = $derived(
    timelineStateValue.current.sessionId === selectedSessionId
      ? timelineItemsToChatMessages(timelineStateValue.current.items, true, tValue.current)
      : [],
  );

  const selectedTurns = $derived(
    sessionDetailValue.current?.session.session_id === selectedSessionId
      ? sessionDetailValue.current.turns
      : [],
  );

  const selectedCompactionEvents = $derived([
    ...(sessionDetailValue.current?.session.session_id === selectedSessionId
      ? sessionDetailValue.current.events
      : []),
    ...compactionEvents,
  ]);

  const selectedCompaction = $derived(
    compactionStatus(selectedCompactionEvents, requestedCompaction, tValue.current),
  );

  const eventCheckpoints = $derived(
    selectedSession?.capabilities.compaction_events === true &&
      selectedSession?.capabilities.compaction_history !== true,
  );

  const conversationTimelineItems = $derived(
    eventCheckpoints
      ? compactionEventItems(
          timelineStateValue.current.sessionId === selectedSessionId
            ? timelineStateValue.current.items
            : [],
          selectedCompactionEvents,
        )
      : timelineStateValue.current.sessionId === selectedSessionId
        ? timelineStateValue.current.items
        : [],
  );

  const compactedHistoryNotice = $derived(
    eventCheckpoints &&
      selectedCompactionEvents.some((event) => event.type === "session.compaction_completed"),
  );

  const timelineMessages = $derived(
    mergeLiveOutputMessages(
      transcriptMessages,
      selectedTurns,
      liveOutputOverlays,
      selectedSession?.current_turn_id ?? null,
      !sessionSupportsTimeline(selectedSession),
    ),
  );

  $effect(() => {
    const id = selectedSessionId,
      timeline = timelineMessages;
    untrack(() => reconcileOptimisticMessages(id, timeline));
  });
  $effect(() => {
    const id = selectedSessionId,
      timeline = timelineMessages;
    untrack(() => reconcileInboxSubmissions(id, timeline));
  });
  const messages = $derived(
    inboxSubmissionMessages(
      chatMessagesWithOptimistic(
        timelineMessages,
        optimisticChatQueryValue.current.data?.[selectedSessionId] ?? [],
      ),
      optimisticInboxQueryValue.current.data?.[selectedSessionId] ?? [],
    ),
  );

  const branchActionInputs = $derived(eligibleBranchActionInputs(selectedSession, messages));

  const branchActionMessageIds = $derived(Object.keys(branchActionInputs));

  const timelineUnavailable = $derived(
    timelineStateValue.current.sessionId === selectedSessionId &&
      Boolean(timelineStateValue.current.error) &&
      !(
        timelineStateValue.current.status === "pending" &&
        timelineStateValue.current.items.length > 0
      ),
  );

  const rulerTurns = $derived(
    sessionDetailValue.current?.session.session_id === selectedSessionId
      ? sessionDetailValue.current.turns
      : [],
  );

  const rulerTreeMode = $derived(
    timelineStateValue.current.sessionId === selectedSessionId &&
      timelineStateValue.current.mode === "tree",
  );

  const rulerNavigableTurnIds = $derived(
    navigableRulerTurnIds(
      rulerTurns,
      rulerTreeMode,
      timelineStateValue.current.sessionId === selectedSessionId
        ? timelineStateValue.current.latestTurnId
        : null,
    ),
  );

  const selectedInboxMessages = $derived(
    selectedSessionId && sessionDetailValue.current?.session.session_id === selectedSessionId
      ? sessionDetailValue.current.inboxMessages
      : [],
  );

  const selectedInboxSubmissions = $derived(
    optimisticInboxQueryValue.current.data?.[selectedSessionId] ?? [],
  );

  const visibleInboxMessages = $derived(
    queuedInboxMessages(visibleChatInboxMessages(selectedInboxMessages), selectedInboxSubmissions),
  );

  const pendingInboxSubmissionId = $derived(
    selectedInboxSubmissions.find((submission) => !submission.acceptedMessage)?.localId ?? null,
  );

  const canSend = $derived(canSendSessionMessage(selectedSession, "x") && !submitting);

  $effect(() => {
    if (
      !timelineUnavailable &&
      matchingPromptScrolls(messages, pendingPromptScrolls, selectedSessionId).length
    )
      untrack(() => {
        void scrollSubmittedPromptAfterLayout(selectedSessionId);
      });
  });

  function requestedSessionIdFromLocation(): string {
    const routeId = routeSessionId();
    if (routeId) return routeId;
    const pathMatch = window.location.pathname.match(/\/chat\/([^/?#]+)$/);
    return pathMatch ? decodeURIComponent(pathMatch[1]) : "";
  }

  function eligibleBranchActionInputs(
    session: SessionView | null,
    chatMessages: typeof messages,
  ): Record<string, string> {
    if (!session?.capabilities.branch_control && !session?.capabilities.fork_session) return {};
    const canEditInCurrentState =
      ["idle", "interrupted", "exited"].includes(session.state) ||
      (isExecutingSessionState(session.state) && session.capabilities.interrupt === true);
    if (!session.capabilities.fork_session && !canEditInCurrentState) return {};
    const turns =
      sessionDetailValue.current?.session.session_id === session.session_id
        ? new Map(sessionDetailValue.current.turns.map((turn) => [turn.turn_id, turn]))
        : new Map();
    const seenTurnIds = new Set<string>();
    const eligibleTurnStates = new Set(["completed", "failed", "interrupted"]);

    return Object.fromEntries(
      chatMessages.flatMap((message) => {
        if (message.role !== "user" || message.status !== "sent" || seenTurnIds.has(message.turnId))
          return [];
        seenTurnIds.add(message.turnId);
        const projectedTurn = turns.get(message.turnId);
        if (!projectedTurn || !eligibleTurnStates.has(projectedTurn.state)) return [];
        if (!projectedTurn.input?.summary?.trim() || !message.content.trim()) return [];
        return [[message.id, message.content]];
      }),
    );
  }

  function navigableRulerTurnIds(
    turns: TurnView[],
    treeMode: boolean,
    latestTurnId: string | null,
  ): string[] {
    if (!treeMode) return turns.map((turn) => turn.turn_id);
    if (!latestTurnId) return [];

    const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]));
    const lineage: string[] = [];
    const visited = new Set<string>();
    let turnId: string | null = latestTurnId;
    while (turnId && !visited.has(turnId)) {
      visited.add(turnId);
      lineage.push(turnId);
      turnId = turnsById.get(turnId)?.parent_turn_id ?? null;
    }
    return lineage;
  }

  function chatMessageElement(turnId: string, role: ChatMessageRole): HTMLElement | null {
    return (
      [
        ...document.querySelectorAll<HTMLElement>(
          "[data-chat-message-id][data-chat-turn-id][data-chat-role]",
        ),
      ].find(
        (element) => element.dataset.chatTurnId === turnId && element.dataset.chatRole === role,
      ) ?? null
    );
  }

  async function navigateFromRuler(turnId: string, role: ChatMessageRole): Promise<void> {
    if (!rulerNavigableTurnIds.includes(turnId)) return;
    let target = chatMessageElement(turnId, role);
    const visitedCursors = new Set<string | null>();

    while (!target) {
      const state = get(timelineState);
      if (state.sessionId !== selectedSessionId || !state.hasMore || state.refreshing) return;
      if (visitedCursors.has(state.nextOlderTurnId)) return;
      visitedCursors.add(state.nextOlderTurnId);
      await loadSessionTimeline(selectedSessionId, {
        mode: "more",
        ...(rulerTreeMode ? { topology: true } : {}),
      });
      await tick();
      target = chatMessageElement(turnId, role);
    }

    target.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function currentSelectedSession(): SessionView | null {
    if (!selectedSessionId) return null;
    const detail = get(sessionDetail);
    return detail?.session.session_id === selectedSessionId ? detail.session : null;
  }

  async function refreshSessionGitStatus(session: SessionView | null): Promise<void> {
    const workspaceId = session?.workspace_id;
    if (!workspaceId) return;
    try {
      await refreshWorkspaceGitStatusMutation.mutateAsync(workspaceId);
    } catch {
      // The mutation exposes the error to the metadata UI.
    }
  }

  /** An Agent turn may have changed files; the Git tab reads them again if it shows. */
  function refreshGitAfterTurn(): void {
    const workspaceId = currentSelectedSession()?.workspace_id;
    if (workspaceId) void invalidateWorkspaceGit(workspaceId);
  }

  async function refreshCurrentSessionGitStatus(): Promise<void> {
    await refreshSessionGitStatus(currentSelectedSession());
  }

  async function cancelPendingInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || message.state !== "pending") return;
    inboxActionMessageId = message.message_id;
    actionError = null;
    try {
      await cancelInboxMessage(selectedSessionId, message.message_id);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      inboxActionMessageId = null;
    }
  }

  async function retryFailedInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || !["failed", "unknown"].includes(message.state)) return;
    if (message.state === "unknown" && !window.confirm(tValue.current("chat.duplicateConfirm")))
      return;
    inboxActionMessageId = message.message_id;
    actionError = null;
    try {
      await retryInboxMessage(selectedSessionId, message, message.state === "unknown");
    } catch (error) {
      actionError =
        error instanceof SubmissionUnconfirmedError
          ? null
          : error instanceof Error
            ? error.message
            : String(error);
    } finally {
      inboxActionMessageId = null;
    }
  }

  async function dismissFailedInboxMessage(message: InboxMessageView): Promise<void> {
    if (!selectedSessionId || message.state !== "failed") return;
    inboxActionMessageId = message.message_id;
    actionError = null;
    try {
      await dismissInboxMessage(selectedSessionId, message.message_id);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      inboxActionMessageId = null;
    }
  }

  function projectedTurnForBranchMessage(message: SessionChatMessage) {
    if (!branchActionMessageIds.includes(message.id)) return null;
    return (
      sessionDetailValue.current?.turns.find((turn) => turn.turn_id === message.turnId) ?? null
    );
  }

  async function submitBranchEdit(message: SessionChatMessage, input: string): Promise<boolean> {
    const projectedTurn = projectedTurnForBranchMessage(message);
    if (!selectedSessionId || !projectedTurn || !input.trim() || branchActionSubmitting)
      return false;

    const sessionId = selectedSessionId;
    branchActionSubmitting = true;
    branchActionError = null;
    try {
      const session = currentSelectedSession();
      if (session?.capabilities.fork_session) {
        const existing = pendingHistoryForksValue.current.find(
          (operation) =>
            operation.sourceSessionId === sessionId && operation.turnId === projectedTurn.turn_id,
        );
        if (existing) {
          branchActionError = tValue.current("chat.recoverBeforeEdit");
          return false;
        }
        const operation = await device.historyForks.start(sessionId, projectedTurn.turn_id, input);
        if (
          operation.sessionId &&
          ["session_created", "input_unconfirmed", "input_submitted"].includes(operation.stage)
        ) {
          await navigate(`/chat/${operation.sessionId}`, undefined, { scope: deviceScope });
          return true;
        }
        return false;
      }
      if (session && isExecutingSessionState(session.state)) {
        if (session.capabilities.interrupt !== true) {
          throw new Error(tValue.current("chat.cannotInterrupt"));
        }
        await interruptSession(sessionId);
        await waitForBranchSubmissionReady(sessionId);
      }
      await submitInboxMessage(sessionId, {
        input,
        delivery_policy: "after_idle",
        metadata: { source: "dashboard_chat_branch_edit" },
        branch_target_turn_id: projectedTurn.turn_id,
      });
      return true;
    } catch (error) {
      branchActionError = error instanceof Error ? error.message : String(error);
      return error instanceof SubmissionUnconfirmedError;
    } finally {
      branchActionSubmitting = false;
    }
  }

  async function recoverHistoryFork(operation: HistoryFork): Promise<void> {
    if (branchActionSubmitting) return;
    branchActionSubmitting = true;
    try {
      const result = await device.historyForks.resume(operation);
      if (
        result.sessionId &&
        ["session_created", "input_unconfirmed", "input_submitted"].includes(result.stage)
      ) {
        await navigate(`/chat/${result.sessionId}`, undefined, { scope: deviceScope });
      }
    } catch (error) {
      branchActionError = error instanceof Error ? error.message : String(error);
    } finally {
      branchActionSubmitting = false;
    }
  }

  function editHistoricalMessage(
    message: SessionChatMessage,
    replacementInput: string,
  ): Promise<boolean> {
    return submitBranchEdit(message, replacementInput);
  }

  function nextAnimationFrame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }

  function scrollChatToBottom(): void {
    scrollDocumentToBottom();
    hideScrollButton?.();
  }

  function matchingPromptScrolls(
    chatMessages: SessionChatMessage[],
    requests: PromptScrollRequest[],
    sessionId: string,
  ): PromptScrollRequest[] {
    const matchedTurnIds = new Set<string>();
    return requests.filter((request) => {
      if (request.sessionId !== sessionId) return false;
      const message = chatMessages.find(
        (item) =>
          item.role === "user" &&
          !request.turnIds.has(item.turnId) &&
          !matchedTurnIds.has(item.turnId) &&
          item.content === request.input,
      );
      if (!message) return false;
      matchedTurnIds.add(message.turnId);
      return true;
    });
  }

  async function scrollSubmittedPromptAfterLayout(sessionId: string): Promise<void> {
    if (promptScrollScheduled) return;
    promptScrollScheduled = true;
    await tick();
    await nextAnimationFrame();
    promptScrollScheduled = false;
    if (destroyed || selectedSessionId !== sessionId) return;
    const mountedMessageIds = new Set(
      [
        ...document.querySelectorAll<HTMLElement>('[data-chat-message-id][data-chat-role="user"]'),
      ].map((element) => element.dataset.chatMessageId),
    );
    const rendered = matchingPromptScrolls(
      messages.filter((message) => mountedMessageIds.has(message.id)),
      pendingPromptScrolls,
      sessionId,
    );
    if (!rendered.length) return;
    scrollChatToBottom();
    pendingPromptScrolls = pendingPromptScrolls.filter((request) => !rendered.includes(request));
    for (const request of pendingPromptScrolls) {
      for (const message of messages) request.turnIds.add(message.turnId);
    }
  }

  async function scrollChatToBottomAfterLayout(): Promise<void> {
    await tick();
    await nextAnimationFrame();
    for (let pass = 0; pass < INITIAL_SCROLL_SETTLE_PASSES; pass += 1) {
      scrollChatToBottom();
    }
  }

  function isTerminalTurnEvent(eventType: string): boolean {
    return (
      eventType === "turn.completed" ||
      eventType === "turn.failed" ||
      eventType === "turn.interrupted"
    );
  }

  function isSessionIdleEvent(eventType: string): boolean {
    return eventType === "session.ready" || isTerminalTurnEvent(eventType);
  }

  async function convergeTerminalTurn(turnId: string): Promise<void> {
    const sessionId = selectedSessionId;
    const [, timelineSucceeded] = await Promise.all([
      loadSessionDetail(sessionId, { showLoading: false }),
      refreshSessionTimeline(sessionId, turnId),
    ]);
    if (!timelineSucceeded || selectedSessionId !== sessionId) return;
    const timeline = get(timelineState);
    const detail = get(sessionDetail);
    const terminalTurn =
      detail?.session.session_id === sessionId
        ? detail.turns.find((turn) => turn.turn_id === turnId)
        : null;
    if (!terminalTurn || !["completed", "failed", "interrupted"].includes(terminalTurn.state))
      return;
    if (timeline.sessionId !== sessionId || !timeline.items.some((item) => item.turn_id === turnId))
      return;
    liveOutputOverlays = removeLiveOutputOverlay(liveOutputOverlays, turnId);
  }

  function handleLiveOutputEvent(event: LiveOutputEvent): void {
    if (get(timelineState).historySource === "file") return;
    liveOutputOverlays = applyLiveOutputEvent(liveOutputOverlays, selectedSessionId, event);
  }

  function startSelectedLiveOutput(session: SessionView | null): void {
    const generation = ++liveOutputGeneration;
    closeLiveOutputStream?.();
    closeLiveOutputStream = null;
    const timeline = get(timelineState);
    if (
      session?.capabilities.timeline &&
      (timeline.status === "loading" ||
        timeline.status === "idle" ||
        timeline.historySource === "file")
    ) {
      liveOutputOverlays = {};
      return;
    }
    if (
      !session ||
      session.session_id !== selectedSessionId ||
      session.capabilities.stream_output !== true
    )
      return;
    closeLiveOutputStream = openLiveOutputStream(session.session_id, {
      onEvent: (event) => {
        if (generation === liveOutputGeneration) handleLiveOutputEvent(event);
      },
      onDisconnected: () => {
        if (generation === liveOutputGeneration)
          liveOutputOverlays = markLiveOutputDisconnected(liveOutputOverlays);
      },
    });
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === "history_changed" && streamEvent.session_id === selectedSessionId) {
      ++liveOutputGeneration;
      ++selectedSessionLoadGeneration;
      closeLiveOutputStream?.();
      closeLiveOutputStream = null;
      liveOutputOverlays = {};
      resetTimelineState(selectedSessionId);
      void loadSessionTimeline(selectedSessionId, {
        mode: "rebuild",
        topology: selectedSession?.capabilities.topology === true,
      });
      return;
    }
    if (streamEvent.kind === "session_event") {
      if (streamEvent.event.session_id !== selectedSessionId) return;
      if (isTerminalTurnEvent(streamEvent.event.type)) refreshGitAfterTurn();
      if (streamEvent.event.type.startsWith("session.compaction_")) {
        compactionEvents = [
          ...compactionEvents.filter((event) => event.event_id !== streamEvent.event.event_id),
          streamEvent.event,
        ];
        void loadSessionDetail(selectedSessionId, { showLoading: false });
        if (
          streamEvent.event.type === "session.compaction_completed" &&
          sessionSupportsTimeline(currentSelectedSession())
        ) {
          if (currentSelectedSession()?.capabilities.compaction_history)
            void refreshSessionTimeline(selectedSessionId);
          else void loadSessionTimeline(selectedSessionId, { mode: "rebuild", topology: false });
        }
        return;
      }
      if (
        streamEvent.event.type === "session.model_updated" ||
        streamEvent.event.type === "session.reasoning_effort_updated"
      ) {
        const sessionId = selectedSessionId;
        void loadSessionDetail(sessionId, { showLoading: false }).finally(() => {
          if (
            streamEvent.event.type === "session.reasoning_effort_updated" &&
            reasoningEffortSessionId === sessionId
          ) {
            reasoningEffortBusy = false;
            reasoningEffortSessionId = "";
          }
        });
        return;
      }
      if (!sessionSupportsTimeline(currentSelectedSession())) {
        const id = selectedSessionId;
        void loadSessionDetail(id, { showLoading: false }).then(() => {
          if (selectedSessionId === id && sessionSupportsTimeline(currentSelectedSession())) {
            void refreshSessionTimeline(id);
          }
        });
        return;
      }
      if (isTerminalTurnEvent(streamEvent.event.type) && streamEvent.event.turn_id) {
        void convergeTerminalTurn(streamEvent.event.turn_id);
        return;
      }
      if (isSessionIdleEvent(streamEvent.event.type)) {
        void loadSessionDetail(selectedSessionId, { showLoading: false });
        void refreshSessionTimeline(selectedSessionId, streamEvent.event.turn_id);
        return;
      }
      if (streamEvent.event.type === "turn.started") {
        void loadSessionDetail(selectedSessionId, { showLoading: false });
        void refreshSessionTimeline(selectedSessionId, streamEvent.event.turn_id);
        return;
      }
    }
  }

  function latestProjectedTurnId(): string | null {
    if (
      !sessionDetailValue.current ||
      sessionDetailValue.current.session.session_id !== selectedSessionId
    )
      return null;
    return sessionDetailValue.current.turns.reduce<string | null>(
      (latestTurnId, turn) =>
        latestTurnId === null || turn.turn_id > latestTurnId ? turn.turn_id : latestTurnId,
      null,
    );
  }

  function openNewChat(): void {
    actionError = null;
    clearChatDraft();
    resetTimelineState();
    navigate(isMobileShell() ? "/new" : "/", { workspace: selectedSession?.workspace_id });
  }

  async function selectSessionFromLocation(): Promise<void> {
    if (dashboardScope() !== deviceScope) return;
    const nextSessionId = requestedSessionIdFromLocation();
    if (nextSessionId === selectedSessionId) return;
    closeLiveOutputStream?.();
    closeLiveOutputStream = null;
    liveOutputOverlays = {};
    compactionEvents = [];
    requestedCompaction = null;
    selectedSessionId = nextSessionId;
    gitStatusSessionId = "";
    selectSession(selectedSessionId || null);
    resetTimelineState(selectedSessionId);
    pendingPromptScrolls = [];
    actionError = null;
    branchActionError = null;
    reasoningEffortBusy = false;
    reasoningEffortSessionId = "";
    if (selectedSessionId) {
      autofocusComposer = claimChatEntryAutofocus(`${device.id}:/chat/${selectedSessionId}`);
      await loadSelectedSession(selectedSessionId);
    } else {
      historyObserverEnabled = false;
      initialChatScrollPending = false;
      resetTimelineState();
    }
  }

  function sessionSupportsTimeline(session: SessionView | null): boolean {
    return (
      session?.capabilities.timeline === true || session?.capabilities.compaction_history === true
    );
  }

  function timelineMode(topology: boolean): "linear" | "tree" {
    return topology ? "tree" : "linear";
  }

  function timelineMatches(sessionId: string, topology: boolean | null): boolean {
    const state = get(timelineState);
    if (!hasTimelineSnapshot(state, sessionId) || state.mode === null) return false;
    return topology === null || state.mode === timelineMode(topology);
  }

  function sessionLoadIsCurrent(sessionId: string, generation: number): boolean {
    return (
      !destroyed && selectedSessionId === sessionId && selectedSessionLoadGeneration === generation
    );
  }

  async function rebuildTimeline(
    sessionId: string,
    generation: number,
    topology: boolean,
  ): Promise<void> {
    if (!sessionLoadIsCurrent(sessionId, generation)) return;
    initialChatScrollPending = true;
    resetTimelineState(sessionId);
    await loadSessionTimeline(sessionId, {
      mode: "rebuild",
      latestTurnId: latestProjectedTurnId(),
      ...(topology ? { topology: true } : {}),
    });
    if (!sessionLoadIsCurrent(sessionId, generation)) return;
    await scrollChatToBottomAfterLayout();
    if (!sessionLoadIsCurrent(sessionId, generation)) return;
    initialChatScrollPending = false;
    historyObserverEnabled = true;
  }

  async function revealCachedTimeline(
    sessionId: string,
    generation: number,
    detailPromise: Promise<unknown>,
  ): Promise<void> {
    const cached = get(timelineState);
    initialChatScrollPending = false;
    transcriptResolving = false;
    // The conversation renders only once the Session detail is selected.
    const conversationRendered = currentSelectedSession() ? Promise.resolve() : detailPromise;
    void conversationRendered.then(() => {
      if (sessionLoadIsCurrent(sessionId, generation)) return scrollChatToBottomAfterLayout();
    });
    void refreshSessionTimeline(sessionId, cached.latestTurnId);
    await detailPromise;
    if (!sessionLoadIsCurrent(sessionId, generation)) return;
    const loadedSession = currentSelectedSession();
    if (!loadedSession || !sessionSupportsTimeline(loadedSession)) {
      resetTimelineState(sessionId);
      return;
    }
    const expectedMode = timelineMode(loadedSession.capabilities.topology === true);
    if (get(timelineState).sessionId === sessionId && get(timelineState).mode === expectedMode) {
      historyObserverEnabled = true;
      return;
    }
    await rebuildTimeline(sessionId, generation, expectedMode === "tree");
  }

  async function loadSelectedSession(sessionId: string): Promise<void> {
    const generation = ++selectedSessionLoadGeneration;
    historyObserverEnabled = false;
    const knownSession = currentSelectedSession();
    const knownTopology = knownSession ? knownSession.capabilities.topology === true : null;
    const detailPromise = loadSessionDetail(sessionId);
    try {
      let hasLoadedTimeline = timelineMatches(sessionId, knownTopology);
      if (!hasLoadedTimeline) {
        transcriptResolving = true;
        initialChatScrollPending = false;
        const current = get(timelineState);
        if (current.sessionId && current.sessionId !== sessionId) resetTimelineState(sessionId);
        const restored = await restoreSessionTimeline(
          sessionId,
          knownTopology === null ? {} : { topology: knownTopology },
        );
        if (!sessionLoadIsCurrent(sessionId, generation)) {
          void detailPromise.catch(() => undefined);
          return;
        }
        transcriptResolving = false;
        hasLoadedTimeline = restored && timelineMatches(sessionId, knownTopology);
      }

      if (hasLoadedTimeline) {
        await revealCachedTimeline(sessionId, generation, detailPromise);
        return;
      }

      initialChatScrollPending = true;
      await detailPromise;
      if (!sessionLoadIsCurrent(sessionId, generation)) return;
      const loadedSession = currentSelectedSession();
      if (!loadedSession || !sessionSupportsTimeline(loadedSession)) {
        initialChatScrollPending = false;
        resetTimelineState(sessionId);
        return;
      }
      await loadSessionTimeline(sessionId, {
        mode: "rebuild",
        latestTurnId: latestProjectedTurnId(),
        ...(loadedSession.capabilities.topology === true ? { topology: true } : {}),
      });
      if (!sessionLoadIsCurrent(sessionId, generation)) return;
      await scrollChatToBottomAfterLayout();
      if (!sessionLoadIsCurrent(sessionId, generation)) return;
      initialChatScrollPending = false;
      historyObserverEnabled = true;
    } catch (error) {
      if (sessionLoadIsCurrent(sessionId, generation)) {
        transcriptResolving = false;
        initialChatScrollPending = false;
      } else {
        void detailPromise.catch(() => undefined);
      }
      throw error;
    }
  }

  async function loadEarlierMessages(): Promise<void> {
    if (
      !selectedSessionId ||
      !timelineStateValue.current.hasMore ||
      timelineStateValue.current.refreshing
    )
      return;
    actionError = null;
    try {
      await loadSessionTimeline(selectedSessionId, {
        mode: "more",
        ...(selectedSession?.capabilities.topology === true ? { topology: true } : {}),
      });
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    }
  }

  async function renameSelectedSession(title: string | null): Promise<void> {
    if (!selectedSession || actionBusy) return;
    const sessionId = selectedSessionId;
    const commandInput = chatDraftValue.current;
    actionBusy = true;
    actionError = null;
    try {
      await updateSessionTitle(sessionId, title);
      if (
        selectedSessionId === sessionId &&
        /^\/rename\s/.test(commandInput.trim()) &&
        chatDraftValue.current === commandInput
      )
        clearChatDraft();
    } catch (error) {
      if (selectedSessionId === sessionId)
        actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = false;
    }
  }

  async function changeSelectedReasoningEffort(reasoningEffort: string): Promise<void> {
    if (
      !selectedSession ||
      selectedSession.capabilities.reasoning_effort_control !== true ||
      reasoningEffortBusy ||
      reasoningEffort === selectedSession.reasoning_effort
    )
      return;
    const sessionId = selectedSessionId;
    const commandInput = chatDraftValue.current;
    reasoningEffortBusy = true;
    actionError = null;
    reasoningEffortSessionId = sessionId;
    try {
      await setSessionReasoningEffort(sessionId, reasoningEffort);
      if (selectedSessionId === sessionId && chatDraftValue.current === commandInput)
        clearChatDraft();
    } catch (error) {
      if (selectedSessionId === sessionId) {
        reasoningEffortBusy = false;
        reasoningEffortSessionId = "";
        actionError = error instanceof Error ? error.message : String(error);
      }
    }
  }

  async function compactSelectedSession(): Promise<void> {
    if (
      !selectedSession ||
      actionBusy ||
      compactionDisabledReason(selectedSession, selectedCompaction?.busy ?? false, tValue.current)
    )
      return;
    const sessionId = selectedSessionId;
    actionBusy = true;
    actionError = null;
    try {
      const receipt = await compactSession(sessionId);
      if (selectedSessionId !== sessionId) return;
      requestedCompaction = receipt.operation_id;
      if (chatDraftValue.current.trim() === "/compact") clearChatDraft();
      await loadSessionDetail(sessionId, { showLoading: false });
      if (
        selectedSessionId === sessionId &&
        sessionDetailValue.current?.events.some(
          (event) =>
            event.type === "session.compaction_completed" &&
            event.payload.operation_id === receipt.operation_id,
        )
      ) {
        await refreshSessionTimeline(sessionId);
      }
    } catch (error) {
      if (selectedSessionId === sessionId)
        actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = false;
    }
  }

  async function exitSelectedSession(): Promise<void> {
    if (!selectedSession || isTerminalSession(selectedSession) || actionBusy) return;
    const sessionId = selectedSessionId;
    actionBusy = true;
    actionError = null;
    try {
      await terminateSession(sessionId);
      if (selectedSessionId === sessionId && chatDraftValue.current.trim() === "/exit")
        clearChatDraft();
    } catch (error) {
      if (selectedSessionId === sessionId)
        actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = false;
    }
  }

  async function toggleSelectedSessionManagement(command: "/pin" | "/archive"): Promise<void> {
    if (!selectedSession || actionBusy) return;
    const session = selectedSession;
    const sessionId = session.session_id;
    actionBusy = true;
    actionError = null;
    try {
      if (command === "/pin") {
        if (session.archived_at) return;
        await (session.pinned_at ? unpinSession : pinSession)(sessionId);
      } else {
        await (session.archived_at ? unarchiveSession : archiveSession)(sessionId);
      }
      if (selectedSessionId === sessionId && chatDraftValue.current.trim() === command)
        clearChatDraft();
    } catch (error) {
      if (selectedSessionId === sessionId)
        actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = false;
    }
  }

  async function interruptSelectedSession(): Promise<void> {
    if (!selectedSessionId) return;
    actionBusy = true;
    actionError = null;
    try {
      await interruptSession(selectedSessionId);
      if (sessionSupportsTimeline(currentSelectedSession())) {
        await refreshSessionTimeline(
          selectedSessionId,
          selectedSession?.current_turn_id ?? latestProjectedTurnId(),
        );
      }
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = false;
    }
  }

  function sessionStateFromDetail(sessionId: string): string | null {
    const detail = get(sessionDetail);
    return detail?.session.session_id === sessionId ? detail.session.state : null;
  }

  function branchSubmissionReadyFromStores(sessionId: string): boolean {
    const detail = get(sessionDetail);
    if (detail?.session.session_id !== sessionId) return false;
    if (!["idle", "interrupted"].includes(detail.session.state)) return false;
    return !detail.turns.some((turn) => turn.state === "queued" || turn.state === "running");
  }

  function waitForBranchSubmissionReady(
    sessionId: string,
    timeoutMs = BRANCH_INTERRUPT_TIMEOUT_MS,
  ): Promise<void> {
    if (branchSubmissionReadyFromStores(sessionId)) return Promise.resolve();

    return new Promise((resolve, reject) => {
      let done = false;
      let unsubscribe: (() => void) | null = null;

      const finish = (callback: () => void) => {
        if (done) return;
        done = true;
        unsubscribe?.();
        clearTimeout(timeout);
        callback();
      };
      const check = () => {
        if (branchSubmissionReadyFromStores(sessionId)) finish(resolve);
      };
      const timeout = setTimeout(() => {
        finish(() => reject(new Error(tValue.current("chat.interruptTimeout"))));
      }, timeoutMs);

      const stop = sessionDetail.subscribe(check);
      unsubscribe = stop;
      if (done) stop();
      else check();
    });
  }

  function waitForSessionIdle(
    sessionId: string,
    timeoutMs = AUTO_RESUME_IDLE_TIMEOUT_MS,
  ): Promise<void> {
    if (sessionStateFromDetail(sessionId) === "idle") return Promise.resolve();

    return new Promise((resolve, reject) => {
      let done = false;
      let unsubscribeDetail: (() => void) | null = null;

      const cleanup = () => {
        unsubscribeDetail?.();
        clearTimeout(timeout);
      };
      const finish = (callback: () => void) => {
        if (done) return;
        done = true;
        cleanup();
        callback();
      };
      const check = () => {
        if (sessionStateFromDetail(sessionId) === "idle") finish(resolve);
      };
      const timeout = setTimeout(() => {
        finish(() => reject(new Error(tValue.current("chat.resumeTimeout"))));
      }, timeoutMs);

      unsubscribeDetail = sessionDetail.subscribe(check);
      check();
    });
  }

  async function sendMessage(
    message: string,
    deliveryPolicy: InboxDeliveryPolicy,
  ): Promise<boolean> {
    if (!canSend || !selectedSessionId || !message.trim()) return false;
    submitting = true;
    actionError = null;
    const originalDraft = chatDraftValue.current;
    const scrollRequest = {
      sessionId: selectedSessionId,
      input: message,
      turnIds: new Set([
        ...messages.map((item) => item.turnId),
        ...selectedTurns.map((turn) => turn.turn_id),
      ]),
    };
    pendingPromptScrolls = [...pendingPromptScrolls, scrollRequest];
    const waitForResume = selectedSession?.state === "exited";
    if (!waitForResume) clearChatDraft();
    try {
      if (waitForResume) {
        await resumeSession(selectedSessionId);
        if (clientResumeWaitsForIdle(selectedSession)) await waitForSessionIdle(selectedSessionId);
        clearChatDraft();
      }
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: deliveryPolicy,
        metadata: { source: "dashboard_chat" },
      });
      return true;
    } catch (error) {
      pendingPromptScrolls = pendingPromptScrolls.filter((request) => request !== scrollRequest);
      if (!get(chatDraft).trim()) chatDraft.set(originalDraft);
      actionError =
        error instanceof SubmissionUnconfirmedError
          ? null
          : error instanceof Error
            ? error.message
            : String(error);
      return false;
    } finally {
      submitting = false;
    }
  }

  const commands: ChatCommand[] = $derived(
    selectedSession
      ? [
          ...reasoningEfforts.map((effort): ChatCommand => ({
            name: `/reasoning ${effort.id}`,
            description:
              effort.description ||
              tValue.current("effort.commandDescription", { value: effort.name }),
            disabledReason: reasoningEffortBusy
              ? tValue.current("effort.changing")
              : effort.id === selectedSession.reasoning_effort
                ? tValue.current("effort.current", { value: effort.name })
                : undefined,
            run: () => void changeSelectedReasoningEffort(effort.id),
          })),
          {
            name: "/compact",
            description: tValue.current("compaction.description"),
            disabledReason: compactionDisabledReason(
              selectedSession,
              selectedCompaction?.busy ?? false,
              tValue.current,
            ),
            run: compactSelectedSession,
          },
          { name: "/new", description: tValue.current("chat.newInWorkspace"), run: openNewChat },
          {
            name: "/rename",
            description: tValue.current("chat.renameCurrent"),
            run: renameSelectedSession,
          },
          {
            name: "/exit",
            description: tValue.current("chat.endCurrent"),
            disabledReason: isTerminalSession(selectedSession)
              ? tValue.current("chat.alreadyEnded")
              : undefined,
            run: exitSelectedSession,
          },
          {
            name: "/pin",
            description: selectedSession.pinned_at
              ? tValue.current("chat.unpinCurrent")
              : tValue.current("chat.pinCurrent"),
            disabledReason: selectedSession.archived_at
              ? tValue.current("chat.archivedCannotPin")
              : undefined,
            active: Boolean(selectedSession.pinned_at),
            run: () => toggleSelectedSessionManagement("/pin"),
          },
          {
            name: "/archive",
            description: selectedSession.archived_at
              ? tValue.current("chat.unarchiveCurrent")
              : tValue.current("chat.archiveCurrent"),
            active: Boolean(selectedSession.archived_at),
            run: () => toggleSelectedSessionManagement("/archive"),
          },
        ]
      : [],
  );

  return {
    get commands() {
      return commands;
    },
    get actionBusy() {
      return actionBusy;
    },
    get actionError() {
      return actionError;
    },
    get autofocusComposer() {
      return autofocusComposer;
    },
    get branchActionError() {
      return branchActionError;
    },
    get branchActionInputs() {
      return branchActionInputs;
    },
    get branchActionSubmitting() {
      return branchActionSubmitting;
    },
    get canSend() {
      return canSend;
    },
    cancelPendingInboxMessage,
    chatDraft,
    compactSelectedSession,
    get compactedHistoryNotice() {
      return compactedHistoryNotice;
    },
    get controlDetails() {
      return controlDetails;
    },
    get conversationTimelineItems() {
      return conversationTimelineItems;
    },
    device,
    pendingHistoryForks,
    dismissFailedInboxMessage,
    editHistoricalMessage,
    exitSelectedSession,
    get historyObserverEnabled() {
      return historyObserverEnabled;
    },
    get inboxActionMessageId() {
      return inboxActionMessageId;
    },
    get initialChatScrollPending() {
      return initialChatScrollPending;
    },
    interruptSelectedSession,
    get liveOutputOverlays() {
      return liveOutputOverlays;
    },
    loadEarlierMessages,
    loadSelectedSession,
    get messages() {
      return messages;
    },
    navigateFromRuler,
    openNewChat,
    get pendingInboxSubmissionId() {
      return pendingInboxSubmissionId;
    },
    recoverHistoryFork,
    refreshCurrentSessionGitStatus,
    refreshSessionTimeline,
    renameSelectedSession,
    retryFailedInboxMessage,
    get rulerNavigableTurnIds() {
      return rulerNavigableTurnIds;
    },
    get rulerTreeMode() {
      return rulerTreeMode;
    },
    get rulerTurns() {
      return rulerTurns;
    },
    scrollChatToBottom,
    get selectedCompaction() {
      return selectedCompaction;
    },
    get selectedSession() {
      return selectedSession;
    },
    get selectedSessionGitStatus() {
      return selectedSessionGitStatus;
    },
    get selectedSessionId() {
      return selectedSessionId;
    },
    get selectedSessionMetadataItems() {
      return selectedSessionMetadataItems;
    },
    get selectedSessionMetadataSummary() {
      return selectedSessionMetadataSummary;
    },
    get selectedTurns() {
      return selectedTurns;
    },
    sendMessage,
    sessionDetailError,
    sessionDetailErrorKind,
    sessionDetailLoading,
    sessionSupportsTimeline,
    get submitting() {
      return submitting;
    },
    timelineState,
    get timelineUnavailable() {
      return timelineUnavailable;
    },
    toggleSelectedSessionManagement,
    get transcriptResolving() {
      return transcriptResolving;
    },
    get visibleInboxMessages() {
      return visibleInboxMessages;
    },
    setHideScrollButton(callback?: () => void) {
      hideScrollButton = callback;
    },
  };
}
export type SessionChatController = ReturnType<typeof createSessionChat>;
