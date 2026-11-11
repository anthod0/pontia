import { QueryObserver } from "@tanstack/svelte-query";
import { get, type Subscriber, type Unsubscriber, type Updater, type Writable } from "svelte/store";

import { ApiError } from "../api/errors";

import type {
  TurnTimelineGroup,
  TurnTimelineItem,
  TurnTimelinePage,
  TurnTreeHistoryPage,
} from "../api/types";

import { readCachedTimeline, writeCachedTimeline } from "../lib/session-chat/timelineCache";
import type { DeviceRuntime } from "../device/runtime";

export type TimelineRefreshKind = "history" | "tail" | null;

export type TimelineStatus =
  | "idle"
  | "loading"
  | "ready"
  | "empty"
  | "query_error"
  | "capability_unavailable"
  | "range_unavailable"
  | "range_invalid"
  | "source_unavailable"
  | "pending"
  | "source_identity_mismatch"
  | "topology_unavailable"
  | "error";

export interface TimelineState {
  sessionId: string;
  historySource?: "native" | "file" | null;
  mode: "linear" | "tree" | null;
  groups: TurnTimelineGroup[];
  items: TurnTimelineItem[];
  nextOlderTurnId: string | null;
  latestTurnId: string | null;
  hasMore: boolean;
  loading: boolean;
  refreshing: boolean;
  refreshKind: TimelineRefreshKind;
  status: TimelineStatus;
  errorCode: string | null;
  error: string | null;
  compactionHistoryError?: string | null;
}

type LoadMode = "rebuild" | "more" | "refresh";

export function hasTimelineSnapshot(state: TimelineState, sessionId: string): boolean {
  return (
    state.sessionId === sessionId &&
    (state.status === "ready" ||
      state.status === "empty" ||
      (state.status === "pending" && state.items.length > 0))
  );
}

type TimelineUpdateQueue = {
  promise: Promise<boolean>;
  resolve: (succeeded: boolean) => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout> | null;
  running: boolean;
  dirty: boolean;
  turnId: string | null;
  succeeded: boolean;
};
export function createTimeline(device: DeviceRuntime) {
  const DEFAULT_HISTORY_LIMIT = 20;

  const FORWARD_LIMIT = 100;

  const TIMELINE_UPDATE_DEBOUNCE_MS = 100;

  let pendingRetry: ReturnType<typeof setTimeout> | null = null;

  let timelineGeneration = 0;

  function emptyState(sessionId = ""): TimelineState {
    return {
      sessionId,
      mode: null,
      groups: [],
      items: [],
      nextOlderTurnId: null,
      latestTurnId: null,
      hasMore: false,
      loading: false,
      refreshing: false,
      refreshKind: null,
      status: "idle",
      errorCode: null,
      error: null,
    };
  }

  let activeTimelineSessionId = "";

  const timelineSubscribers = new Set<Subscriber<TimelineState>>();
  let snapshotObserver: QueryObserver<TimelineState> | null = null;

  function snapshotOptions() {
    return {
      queryKey: device.timelineQueries.timelineKeys.snapshot(activeTimelineSessionId),
      enabled: false,
    };
  }

  function isTimelineRequestForSession(queryKey: readonly unknown[], sessionId: string): boolean {
    return (
      queryKey[1] === "sessions" &&
      queryKey[2] === "timeline" &&
      queryKey[3] === "request" &&
      queryKey[5] === sessionId
    );
  }

  function storedTimelineState(state: TimelineState): TimelineState {
    return { ...state, loading: false, refreshing: false };
  }

  function currentTimelineState(): TimelineState {
    const state =
      device.queryClient.getQueryData<TimelineState>(
        device.timelineQueries.timelineKeys.snapshot(activeTimelineSessionId),
      ) ?? emptyState(activeTimelineSessionId);
    const fetching =
      device.queryClient.isFetching({
        predicate: (query) => isTimelineRequestForSession(query.queryKey, activeTimelineSessionId),
      }) > 0;
    return {
      ...state,
      loading: fetching && state.status === "loading" && state.refreshKind === null,
      refreshing: fetching && state.refreshKind !== null,
    };
  }

  function notifyTimelineSubscribers(): void {
    const state = currentTimelineState();
    for (const subscriber of timelineSubscribers) subscriber(state);
  }

  device.queryClient.getQueryCache().subscribe((event) => {
    const key = event.query.queryKey;
    const isActiveSnapshot =
      key.length === 5 &&
      key[1] === "sessions" &&
      key[2] === "timeline" &&
      key[3] === "snapshot" &&
      key[4] === activeTimelineSessionId;
    if (isActiveSnapshot || isTimelineRequestForSession(key, activeTimelineSessionId)) {
      notifyTimelineSubscribers();
    }
  });

  const timelineState: Writable<TimelineState> = {
    subscribe(run: Subscriber<TimelineState>): Unsubscriber {
      if (!snapshotObserver) {
        snapshotObserver = new QueryObserver<TimelineState>(device.queryClient, snapshotOptions());
        snapshotObserver.subscribe(notifyTimelineSubscribers);
      }
      timelineSubscribers.add(run);
      run(currentTimelineState());
      return () => {
        timelineSubscribers.delete(run);
        if (!timelineSubscribers.size) {
          snapshotObserver?.destroy();
          snapshotObserver = null;
        }
      };
    },
    set(state: TimelineState): void {
      activeTimelineSessionId = state.sessionId;
      snapshotObserver?.setOptions(snapshotOptions());
      device.queryClient.setQueryData(
        device.timelineQueries.timelineKeys.snapshot(state.sessionId),
        storedTimelineState(state),
      );
    },
    update(update: Updater<TimelineState>): void {
      timelineState.set(update(currentTimelineState()));
    },
  };

  async function restoreSessionTimeline(
    sessionId: string,
    options: { topology?: boolean } = {},
  ): Promise<boolean> {
    const generation = timelineGeneration;
    const snapshot = await readCachedTimeline(device.id, sessionId);
    if (generation !== timelineGeneration) return false;
    const current = get(timelineState);
    const expectedMode =
      options.topology === undefined ? null : options.topology ? "tree" : "linear";
    if (
      (current.sessionId && current.sessionId !== sessionId) ||
      !snapshot ||
      snapshot.historySource != null ||
      (expectedMode !== null && snapshot.mode !== expectedMode)
    )
      return false;

    timelineState.set({
      sessionId,
      mode: snapshot.mode,
      groups: snapshot.groups,
      items: snapshot.items,
      nextOlderTurnId: snapshot.nextOlderTurnId,
      latestTurnId: snapshot.latestTurnId,
      hasMore: snapshot.hasMore,
      loading: false,
      refreshing: false,
      refreshKind: null,
      status: statusForItems(snapshot.items),
      errorCode: null,
      error: null,
    });
    if (snapshot.latestTurnId) cachedRefreshCursorSessions.add(sessionId);
    if (snapshot.hasMore && snapshot.nextOlderTurnId) cachedHistoryCursorSessions.add(sessionId);
    return true;
  }

  const timelineUpdateQueues = new Map<string, TimelineUpdateQueue>();

  const cachedRefreshCursorSessions = new Set<string>();

  const cachedHistoryCursorSessions = new Set<string>();

  function resetTimelineState(sessionId = ""): void {
    timelineGeneration += 1;
    if (activeTimelineSessionId)
      device.timelineQueries.cancelTimelineRequests(activeTimelineSessionId);
    if (pendingRetry) clearTimeout(pendingRetry);
    pendingRetry = null;
    timelineState.set(emptyState(sessionId));
    if (sessionId) {
      cachedRefreshCursorSessions.delete(sessionId);
      cachedHistoryCursorSessions.delete(sessionId);
    } else {
      cachedRefreshCursorSessions.clear();
      cachedHistoryCursorSessions.clear();
    }
    for (const queuedSessionId of timelineUpdateQueues.keys())
      clearTimelineUpdateQueue(queuedSessionId);
  }

  function uniqueTimelineItems(items: TurnTimelineItem[]): TurnTimelineItem[] {
    const seen = new Set<string>();
    return items.filter((item) => {
      if (seen.has(item.item_id)) return false;
      seen.add(item.item_id);
      return true;
    });
  }

  function prependUnseenTurnGroups(
    currentItems: TurnTimelineItem[],
    previousItems: TurnTimelineItem[],
  ): TurnTimelineItem[] {
    const currentTurnIds = new Set(
      currentItems.flatMap((item) => (item.turn_id ? [item.turn_id] : [])),
    );
    return uniqueTimelineItems([
      ...previousItems.filter((item) => !item.turn_id || !currentTurnIds.has(item.turn_id)),
      ...currentItems,
    ]);
  }

  function replaceReturnedTurnGroups(
    currentItems: TurnTimelineItem[],
    returnedItems: TurnTimelineItem[],
  ): TurnTimelineItem[] {
    const replacement = uniqueTimelineItems(returnedItems);
    const returnedTurnIds = new Set(
      replacement.flatMap((item) => (item.turn_id ? [item.turn_id] : [])),
    );
    if (!replacement.length) return currentItems;

    const firstReturnedIndex = currentItems.findIndex(
      (item) => item.turn_id !== null && returnedTurnIds.has(item.turn_id),
    );
    if (firstReturnedIndex < 0) return uniqueTimelineItems([...currentItems, ...replacement]);
    let start = firstReturnedIndex;
    while (start > 0 && currentItems[start - 1].turn_id === null) start--;
    return uniqueTimelineItems([...currentItems.slice(0, start), ...replacement]);
  }

  function prependUnseenGroups(
    currentGroups: TurnTimelineGroup[],
    previousGroups: TurnTimelineGroup[],
  ): TurnTimelineGroup[] {
    const currentTurnIds = new Set(currentGroups.map((group) => group.turn_id));
    return [
      ...previousGroups.filter((group) => !currentTurnIds.has(group.turn_id)),
      ...currentGroups,
    ];
  }

  function applyTreeUpdates(
    currentGroups: TurnTimelineGroup[],
    retainThroughTurnId: string | null,
    returnedGroups: TurnTimelineGroup[],
  ): TurnTimelineGroup[] {
    if (retainThroughTurnId === null) return returnedGroups;
    const retainIndex = currentGroups.findIndex((group) => group.turn_id === retainThroughTurnId);
    if (retainIndex < 0) return returnedGroups;
    const retained = currentGroups.slice(0, retainIndex + 1);
    if (returnedGroups[0]?.turn_id === retainThroughTurnId) retained.pop();
    return [...retained, ...returnedGroups];
  }

  function statusForItems(items: TurnTimelineItem[]): TimelineStatus {
    return items.length ? "ready" : "empty";
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function errorStatus(error: unknown): TimelineStatus {
    if (!(error instanceof ApiError)) return "error";
    switch (error.code) {
      case "invalid_timeline_query":
        return "query_error";
      case "timeline_capability_unavailable":
        return "capability_unavailable";
      case "timeline_native_association_unavailable":
      case "turn_timeline_unavailable":
        return "range_unavailable";
      case "turn_timeline_invalid":
        return "range_invalid";
      case "timeline_pending":
        return "pending";
      case "timeline_source_identity_mismatch":
        return "source_identity_mismatch";
      case "timeline_source_unavailable":
        return "source_unavailable";
      case "turn_topology_unknown":
      case "turn_topology_invalid":
        return "topology_unavailable";
      default:
        return "error";
    }
  }

  function isPendingHistory(error: unknown): boolean {
    return error instanceof ApiError && error.code === "timeline_pending";
  }

  function applyTimelineError(
    sessionId: string,
    error: unknown,
    retry: () => Promise<unknown>,
  ): void {
    const previous = get(timelineState);
    if (previous.sessionId !== sessionId) return;
    if (pendingRetry) clearTimeout(pendingRetry);
    pendingRetry = null;
    const pending = isPendingHistory(error);
    if (pending) {
      pendingRetry = setTimeout(() => {
        pendingRetry = null;
        const current = get(timelineState);
        if (current.sessionId === sessionId && current.status === "pending") {
          void retry();
        }
      }, 1000);
    }
    timelineState.update((state) =>
      state.sessionId !== sessionId
        ? state
        : {
            ...(pending || state.historySource ? state : emptyState(sessionId)),
            refreshing: false,
            refreshKind: null,
            loading: false,
            status: errorStatus(error),
            errorCode: error instanceof ApiError ? error.code : "request_failed",
            error: errorMessage(error),
          },
    );
  }

  async function loadSessionTimeline(
    sessionId: string,
    options: {
      mode?: LoadMode;
      limit?: number;
      latestTurnId?: string | null;
      topology?: boolean;
    } = {},
  ): Promise<TurnTimelinePage | TurnTreeHistoryPage | null> {
    if (!sessionId) {
      resetTimelineState();
      return null;
    }

    const mode = options.mode ?? "rebuild";
    if (mode === "rebuild") {
      resetTimelineState(sessionId);
    }
    const current = get(timelineState);
    const sameSession = current.sessionId === sessionId;
    if (!sameSession) timelineGeneration += 1;
    const generation = timelineGeneration;
    const topology = options.topology ?? (sameSession && current.mode === "tree");
    const turnId = mode === "more" && sameSession ? current.nextOlderTurnId : null;
    if (mode === "more" && !turnId) return null;

    timelineState.update((state) => {
      const scoped = state.sessionId === sessionId ? state : emptyState(sessionId);
      return {
        ...scoped,
        mode: topology ? "tree" : "linear",
        latestTurnId: topology
          ? scoped.latestTurnId
          : (options.latestTurnId ?? scoped.latestTurnId),
        loading: mode === "rebuild",
        refreshing: mode === "more",
        refreshKind: mode === "more" ? "history" : mode === "refresh" ? "tail" : null,
        status: mode === "rebuild" ? "loading" : scoped.status,
        errorCode: null,
        error: null,
      };
    });

    try {
      if (topology) {
        const page = await device.timelineQueries.fetchTurnTreeHistory(
          sessionId,
          turnId,
          options.limit ?? DEFAULT_HISTORY_LIMIT,
        );
        if (generation !== timelineGeneration) return null;
        const groups =
          mode === "more"
            ? prependUnseenGroups(get(timelineState).groups, page.groups)
            : page.groups;
        const { items, error: compactionHistoryError } =
          await device.compactionHistory.treeTimelineWithCheckpoints(
            sessionId,
            groups,
            options.limit ?? DEFAULT_HISTORY_LIMIT,
          );
        if (generation !== timelineGeneration) return null;
        timelineState.update((state) => {
          if (state.sessionId !== sessionId) return state;
          return {
            ...state,
            groups,
            items,
            compactionHistoryError,
            nextOlderTurnId: page.next_from_turn_id,
            latestTurnId: groups.at(-1)?.turn_id ?? null,
            hasMore: page.next_from_turn_id !== null,
            loading: false,
            refreshing: false,
            refreshKind: null,
            status: statusForItems(items),
            errorCode: null,
            error: null,
          };
        });
        if (mode === "more") cachedHistoryCursorSessions.delete(sessionId);
        await persistTimelineSnapshot(sessionId);
        return page;
      }

      const page = await device.timelineQueries.fetchTurnTimeline(
        sessionId,
        "backward",
        turnId,
        options.limit ?? DEFAULT_HISTORY_LIMIT,
        mode !== "rebuild" ? current.historySource : null,
      );
      if (generation !== timelineGeneration) return null;
      timelineState.update((state) => {
        if (state.sessionId !== sessionId) return state;
        const items =
          mode === "more"
            ? prependUnseenTurnGroups(state.items, page.items)
            : uniqueTimelineItems(page.items);
        return {
          ...state,
          groups: [],
          historySource: page.history_source,
          items,
          nextOlderTurnId: page.next_turn_id,
          latestTurnId:
            page.history_source == null && mode === "rebuild" && options.latestTurnId
              ? options.latestTurnId
              : (items.findLast((item) => item.turn_id !== null)?.turn_id ?? state.latestTurnId),
          hasMore: page.next_turn_id !== null,
          loading: false,
          refreshing: false,
          refreshKind: null,
          status: statusForItems(items),
          errorCode: null,
          error: null,
        };
      });
      if (mode === "more") cachedHistoryCursorSessions.delete(sessionId);
      await persistTimelineSnapshot(sessionId);
      return page;
    } catch (error) {
      if (generation !== timelineGeneration) return null;
      if (
        !isPendingHistory(error) &&
        mode === "more" &&
        cachedHistoryCursorSessions.has(sessionId)
      ) {
        if (isStaleCachedCursorError(error)) {
          cachedHistoryCursorSessions.delete(sessionId);
          cachedRefreshCursorSessions.delete(sessionId);
          return loadSessionTimeline(sessionId, { mode: "rebuild", topology });
        }
        retainCachedTimelineAfterError(sessionId);
        return null;
      }
      applyTimelineError(sessionId, error, () =>
        loadSessionTimeline(sessionId, { ...options, mode, topology }),
      );
      return null;
    }
  }

  async function loadForwardPages(
    sessionId: string,
    initialTurnId: string,
    generation: number,
  ): Promise<TurnTimelineItem[]> {
    const items: TurnTimelineItem[] = [];
    const seenAnchors = new Set<string>();
    let turnId: string | null = initialTurnId;

    while (turnId && generation === timelineGeneration) {
      if (seenAnchors.has(turnId))
        throw new Error("Turn timeline pagination returned a repeated anchor");
      seenAnchors.add(turnId);
      const page = await device.timelineQueries.fetchTurnTimeline(
        sessionId,
        "forward",
        turnId,
        FORWARD_LIMIT,
        get(timelineState).historySource,
      );
      items.push(...page.items);
      turnId = page.next_turn_id;
    }

    return items;
  }

  async function refreshSessionTimelineUpdates(
    sessionId: string,
    latestTurnId: string,
  ): Promise<boolean> {
    const generation = timelineGeneration;
    timelineState.update((state) => ({
      ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
      refreshing: true,
      refreshKind: "tail",
      errorCode: null,
      error: null,
    }));

    try {
      const updates = await loadForwardPages(sessionId, latestTurnId, generation);
      if (generation !== timelineGeneration) return false;
      timelineState.update((state) => {
        if (state.sessionId !== sessionId) return state;
        const items = replaceReturnedTurnGroups(state.items, updates);
        return {
          ...state,
          items,
          latestTurnId: items.findLast((item) => item.turn_id !== null)?.turn_id ?? latestTurnId,
          loading: false,
          refreshing: false,
          refreshKind: null,
          status: statusForItems(items),
          errorCode: null,
          error: null,
        };
      });
      cachedRefreshCursorSessions.delete(sessionId);
      await persistTimelineSnapshot(sessionId);
      return true;
    } catch (error) {
      if (generation !== timelineGeneration) return false;
      if (!isPendingHistory(error) && cachedRefreshCursorSessions.has(sessionId)) {
        if (isStaleCachedCursorError(error)) {
          cachedRefreshCursorSessions.delete(sessionId);
          cachedHistoryCursorSessions.delete(sessionId);
          return (await loadSessionTimeline(sessionId, { mode: "rebuild" })) !== null;
        }
        retainCachedTimelineAfterError(sessionId);
        return false;
      }
      applyTimelineError(sessionId, error, () =>
        refreshSessionTimelineUpdates(sessionId, latestTurnId),
      );
      return false;
    }
  }

  async function refreshSessionTreeUpdates(
    sessionId: string,
    latestTurnId: string,
  ): Promise<boolean> {
    const generation = timelineGeneration;
    timelineState.update((state) => ({
      ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
      mode: "tree",
      refreshing: true,
      refreshKind: "tail",
      errorCode: null,
      error: null,
    }));

    try {
      const updates = await device.timelineQueries.fetchTurnTreeUpdates(sessionId, latestTurnId);
      if (generation !== timelineGeneration) return false;
      const groups = applyTreeUpdates(
        get(timelineState).groups,
        updates.retain_through_turn_id,
        updates.groups,
      );
      const { items, error: compactionHistoryError } =
        await device.compactionHistory.treeTimelineWithCheckpoints(
          sessionId,
          groups,
          DEFAULT_HISTORY_LIMIT,
        );
      if (generation !== timelineGeneration) return false;
      timelineState.update((state) => {
        if (state.sessionId !== sessionId) return state;
        return {
          ...state,
          groups,
          items,
          compactionHistoryError,
          latestTurnId: updates.current_turn_id,
          nextOlderTurnId: groups[0]?.parent_turn_id ?? null,
          hasMore: groups[0]?.parent_turn_id != null,
          loading: false,
          refreshing: false,
          refreshKind: null,
          status: statusForItems(items),
          errorCode: null,
          error: null,
        };
      });
      cachedRefreshCursorSessions.delete(sessionId);
      await persistTimelineSnapshot(sessionId);
      return true;
    } catch (error) {
      if (generation !== timelineGeneration) return false;
      if (!isPendingHistory(error) && cachedRefreshCursorSessions.has(sessionId)) {
        if (isStaleCachedCursorError(error)) {
          cachedRefreshCursorSessions.delete(sessionId);
          cachedHistoryCursorSessions.delete(sessionId);
          return (
            (await loadSessionTimeline(sessionId, { mode: "rebuild", topology: true })) !== null
          );
        }
        retainCachedTimelineAfterError(sessionId);
        return false;
      }
      applyTimelineError(sessionId, error, () =>
        refreshSessionTreeUpdates(sessionId, latestTurnId),
      );
      return false;
    }
  }

  function refreshSessionTimeline(
    sessionId: string,
    turnId: string | null = null,
  ): Promise<boolean> {
    const existing = timelineUpdateQueues.get(sessionId);
    if (existing) {
      existing.dirty = true;
      if (turnId) existing.turnId = turnId;
      if (!existing.running) scheduleTimelineUpdate(sessionId, existing);
      return existing.promise;
    }

    let resolveQueue: (succeeded: boolean) => void = () => {};
    let rejectQueue: (error: unknown) => void = () => {};
    const queue: TimelineUpdateQueue = {
      promise: new Promise<boolean>((resolve, reject) => {
        resolveQueue = resolve;
        rejectQueue = reject;
      }),
      resolve: resolveQueue,
      reject: rejectQueue,
      timer: null,
      running: false,
      dirty: true,
      turnId,
      succeeded: true,
    };
    timelineUpdateQueues.set(sessionId, queue);
    scheduleTimelineUpdate(sessionId, queue);
    return queue.promise;
  }

  function scheduleTimelineUpdate(sessionId: string, queue: TimelineUpdateQueue): void {
    if (queue.timer) clearTimeout(queue.timer);
    queue.timer = setTimeout(() => {
      queue.timer = null;
      void runTimelineUpdateQueue(sessionId, queue);
    }, TIMELINE_UPDATE_DEBOUNCE_MS);
  }

  async function runTimelineUpdateQueue(
    sessionId: string,
    queue: TimelineUpdateQueue,
  ): Promise<void> {
    if (queue.running) return;
    queue.running = true;
    queue.dirty = false;
    try {
      queue.succeeded = await refreshSessionTimelineNow(sessionId, queue.turnId);
      queue.running = false;
      if (queue.dirty && timelineUpdateQueues.get(sessionId) === queue) {
        scheduleTimelineUpdate(sessionId, queue);
        return;
      }
      if (timelineUpdateQueues.get(sessionId) === queue) timelineUpdateQueues.delete(sessionId);
      queue.resolve(queue.succeeded);
    } catch (error) {
      queue.running = false;
      if (timelineUpdateQueues.get(sessionId) === queue) timelineUpdateQueues.delete(sessionId);
      queue.reject(error);
    }
  }

  function clearTimelineUpdateQueue(sessionId: string): void {
    const queue = timelineUpdateQueues.get(sessionId);
    if (!queue) return;
    if (queue.timer) clearTimeout(queue.timer);
    timelineUpdateQueues.delete(sessionId);
    if (!queue.running) queue.resolve(false);
  }

  function retainCachedTimelineAfterError(sessionId: string): void {
    timelineState.update((state) =>
      state.sessionId !== sessionId
        ? state
        : {
            ...state,
            loading: false,
            refreshing: false,
            refreshKind: null,
            status: statusForItems(state.items),
            errorCode: null,
            error: null,
          },
    );
  }

  function isStaleCachedCursorError(error: unknown): boolean {
    return (
      error instanceof ApiError &&
      (error.code === "turn_timeline_unavailable" ||
        error.code === "turn_timeline_invalid" ||
        error.code === "turn_topology_unknown" ||
        error.code === "turn_topology_invalid")
    );
  }

  async function persistTimelineSnapshot(sessionId: string): Promise<void> {
    const state = get(timelineState);
    if (!hasTimelineSnapshot(state, sessionId) || state.mode === null) return;
    await writeCachedTimeline({
      deviceId: device.id,
      sessionId,
      mode: state.mode,
      historySource: state.historySource,
      groups: state.groups,
      items: state.items,
      nextOlderTurnId: state.nextOlderTurnId,
      latestTurnId: state.latestTurnId,
      hasMore: state.hasMore,
    });
  }

  async function refreshSessionTimelineNow(
    sessionId: string,
    turnId: string | null,
  ): Promise<boolean> {
    const current = get(timelineState);
    if (current.sessionId && current.sessionId !== sessionId) return false;

    if (current.mode === "tree") {
      if (!current.latestTurnId) {
        return (await loadSessionTimeline(sessionId, { mode: "rebuild", topology: true })) !== null;
      }
      return refreshSessionTreeUpdates(sessionId, current.latestTurnId);
    }

    const forwardAnchorTurnId = current.historySource
      ? current.latestTurnId
      : (turnId ?? current.latestTurnId);
    if (!forwardAnchorTurnId) {
      return (
        (await loadSessionTimeline(sessionId, {
          mode: current.historySource ? "refresh" : "rebuild",
        })) !== null
      );
    }
    return refreshSessionTimelineUpdates(sessionId, forwardAnchorTurnId);
  }
  return {
    timelineState,
    restoreSessionTimeline,
    resetTimelineState,
    loadSessionTimeline,
    refreshSessionTimeline,
  };
}
