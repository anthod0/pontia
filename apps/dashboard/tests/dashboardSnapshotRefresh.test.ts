import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { TurnTimelineItem } from "../src/api/types";
import type { SessionConsoleDetail } from "../src/stores/sessions";
import { createSnapshotRefresh } from "../src/services/dashboardSnapshotRefresh";
import { testDevice } from "./deviceFixture";

const { timeline, sessions } = testDevice;
let snapshotRefresh: ReturnType<typeof createSnapshotRefresh>;

function detail(topology: boolean): SessionConsoleDetail {
  return {
    session: {
      session_id: "session-1",
      client_type: "pi",
      title: null,
      handle: null,
      role: null,
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: null,
      workspace: null,
      capabilities: { timeline: true, topology },
      model: null,
      context_usage: null,
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T00:00:00Z",
      metadata: {},
    },
    turns: [],
    inboxMessages: [],
    events: [],
  };
}

function item(turnId: string, content: string): TurnTimelineItem {
  return {
    item_id: `item-${turnId}`,
    turn_id: turnId,
    kind: "assistant",
    role: "assistant",
    title: null,
    status: null,
    occurred_at: null,
    content_preview: content,
  };
}

beforeEach(() => {
  snapshotRefresh = createSnapshotRefresh(testDevice);
  timeline.resetTimelineState();
  sessions.selectSession("session-1");
  vi.spyOn(testDevice.invalidation, "invalidateQueriesAfterConnectionRecovery").mockResolvedValue();
  vi.spyOn(testDevice.workflowQueries, "fetchWorkflows").mockResolvedValue([]);
});

afterEach(() => {
  timeline.resetTimelineState();
  sessions.selectSession(null);
  vi.restoreAllMocks();
});

test.each([false, true])(
  "connection recovery retains loaded history until updates arrive (topology: %s)",
  async (topology) => {
    const older = item("turn-1", "Earlier answer");
    const latest = item("turn-2", "Current answer");
    const updated = item("turn-2", "Updated answer");
    timeline.timelineState.set({
      ...get(timeline.timelineState),
      sessionId: "session-1",
      mode: topology ? "tree" : "linear",
      groups: [
        { turn_id: "turn-1", parent_turn_id: null, state: "completed", items: [older] },
        { turn_id: "turn-2", parent_turn_id: "turn-1", state: "completed", items: [latest] },
      ],
      items: [older, latest],
      latestTurnId: "turn-2",
      status: "ready",
    });
    const pendingDetail = Promise.withResolvers<SessionConsoleDetail>();
    vi.spyOn(sessions, "loadSessionDetail").mockReturnValue(pendingDetail.promise);
    const pendingUpdate = Promise.withResolvers<void>();
    const updateStarted = Promise.withResolvers<void>();
    vi.spyOn(testDevice.api, "getTurnTimeline").mockImplementation(async () => {
      updateStarted.resolve();
      await pendingUpdate.promise;
      return {
        session_id: "session-1",
        direction: "forward",
        items: [updated],
        next_turn_id: null,
      };
    });
    vi.spyOn(testDevice.api, "getTurnTreeUpdates").mockImplementation(async () => {
      updateStarted.resolve();
      await pendingUpdate.promise;
      return {
        session_id: "session-1",
        current_turn_id: "turn-2",
        retain_through_turn_id: "turn-1",
        groups: [
          { turn_id: "turn-2", parent_turn_id: "turn-1", state: "completed", items: [updated] },
        ],
      };
    });

    const refresh = snapshotRefresh.refreshDashboardSnapshot({ reason: "sse_open" });
    expect(get(timeline.timelineState).items).toEqual([older, latest]);
    pendingDetail.resolve(detail(topology));
    await updateStarted.promise;
    expect(get(timeline.timelineState)).toMatchObject({
      items: [older, latest],
      loading: false,
      refreshing: true,
    });
    pendingUpdate.resolve();
    await refresh;
    expect(get(timeline.timelineState).items).toEqual([older, updated]);
  },
);

test("connection recovery loads history when there is no snapshot", async () => {
  vi.spyOn(sessions, "loadSessionDetail").mockResolvedValue(detail(false));
  const loaded = item("turn-1", "Loaded answer");
  vi.spyOn(testDevice.api, "getTurnTimeline").mockResolvedValue({
    session_id: "session-1",
    direction: "backward",
    items: [loaded],
    next_turn_id: null,
  });

  await snapshotRefresh.refreshDashboardSnapshot({ reason: "sse_open" });

  expect(get(timeline.timelineState)).toMatchObject({
    sessionId: "session-1",
    items: [loaded],
    status: "ready",
    loading: false,
  });
});

test("connection recovery does not replace a newly selected session's history", async () => {
  const pendingDetail = Promise.withResolvers<SessionConsoleDetail>();
  vi.spyOn(sessions, "loadSessionDetail").mockReturnValue(pendingDetail.promise);
  const refresh = snapshotRefresh.refreshDashboardSnapshot({ reason: "sse_open" });
  sessions.selectSession("session-2");
  const selectedItems = [item("other-turn", "Other session answer")];
  timeline.timelineState.set({
    ...get(timeline.timelineState),
    sessionId: "session-2",
    items: selectedItems,
    status: "ready",
  });

  pendingDetail.resolve(detail(false));
  await refresh;

  expect(get(timeline.timelineState)).toMatchObject({
    sessionId: "session-2",
    items: selectedItems,
  });
});
