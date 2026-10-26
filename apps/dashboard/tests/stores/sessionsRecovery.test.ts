import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { ApiError } from "../../src/api/errors";
import type { DashboardStreamEvent, SessionView, TurnView } from "../../src/api/types";
import { invalidateQueriesForDashboardEvent } from "../../src/queries/dashboardInvalidation";
import { createDashboardRefreshScheduler } from "../../src/services/dashboardRefreshScheduler";
import * as api from "../../src/api/client";
import {
  loadSessionDetail,
  selectSession,
  selectedSessionId,
  sessionDetail,
  sessionDetailError,
  sessionDetailErrorKind,
  sessionDetailLoading,
} from "../../src/stores/sessions";

const session = (id: string, state = "idle") =>
  ({ session_id: id, state, capabilities: {} }) as SessionView;

function sessionEvent(type: string, sessionId = "a"): DashboardStreamEvent {
  return {
    kind: "session_event",
    id: type,
    occurred_at: "2026-05-14T00:00:00Z",
    event: {
      event_id: type,
      session_id: sessionId,
      turn_id: "turn-1",
      source: "runtime",
      type,
      time: "2026-05-14T00:00:00Z",
      payload: {},
    },
  };
}

beforeEach(() => {
  vi.spyOn(api, "getSession").mockImplementation(async (id) => session(id));
  vi.spyOn(api, "listTurns").mockResolvedValue([]);
  vi.spyOn(api, "listInboxMessages").mockResolvedValue([]);
  vi.spyOn(api, "listEvents").mockResolvedValue([]);
  selectSession("a");
});

afterEach(() => {
  selectSession(null);
  vi.restoreAllMocks();
});

test.each([
  [new TypeError("Failed to fetch"), "network"],
  [new ApiError("Missing session", "not_found", 404), "not_found"],
  [new ApiError("Unauthorized", "unauthorized", 401), "authentication"],
  [new ApiError("Forbidden", "forbidden", 403), "authentication"],
  [new ApiError("Unavailable", "unavailable", 503), "request"],
])("retains the route target and classifies %s as %s", async (error, kind) => {
  vi.mocked(api.getSession).mockRejectedValueOnce(error);
  await loadSessionDetail("a");
  expect(get(selectedSessionId)).toBe("a");
  expect(get(sessionDetail)).toBeNull();
  expect(get(sessionDetailErrorKind)).toBe(kind);
  expect(get(sessionDetailLoading)).toBe(false);

  await loadSessionDetail("a", { showLoading: false });
  expect(get(sessionDetail)?.session.session_id).toBe("a");
  expect(get(sessionDetailError)).toBeNull();
});

test("a missing detail subresource does not claim the Session is missing", async () => {
  vi.mocked(api.listTurns).mockRejectedValueOnce(new ApiError("Missing turns", "not_found", 404));
  await loadSessionDetail("a");
  expect(get(sessionDetailErrorKind)).toBe("request");
});

test("retains an existing snapshot after a background network failure", async () => {
  await loadSessionDetail("a");
  vi.mocked(api.getSession).mockRejectedValueOnce(new TypeError("Failed to fetch"));
  await loadSessionDetail("a", { showLoading: false });
  expect(get(sessionDetail)?.session.session_id).toBe("a");
  expect(get(sessionDetailErrorKind)).toBe("network");
});

test.each(["success", "failure"])(
  "ignores an old route request %s after switching away and back",
  async (outcome) => {
    let finish!: (value: SessionView) => void;
    let fail!: (error: Error) => void;
    let signal: AbortSignal | null | undefined;
    vi.mocked(api.getSession).mockImplementationOnce((_id, options) => {
      signal = options?.signal;
      return new Promise((resolve, reject) => {
        finish = resolve;
        fail = reject;
      });
    });
    const oldRequest = loadSessionDetail("a");
    await invalidateQueriesForDashboardEvent(sessionEvent("turn.completed"));
    selectSession("b");
    await loadSessionDetail("b");
    selectSession("a");
    vi.mocked(api.getSession).mockResolvedValueOnce(session("a", "busy"));
    await loadSessionDetail("a");
    if (outcome === "success") finish(session("a", "idle"));
    else fail(new TypeError("Failed to fetch"));
    await oldRequest;

    expect(signal?.aborted).toBe(true);
    expect(get(sessionDetail)?.session.state).toBe("busy");
    expect(get(sessionDetailError)).toBeNull();
    expect(get(sessionDetailLoading)).toBe(false);
  },
);

test("coalesces concurrent recovery requests and permits a fresh read after failure", async () => {
  let fail!: (error: Error) => void;
  vi.mocked(api.getSession).mockImplementationOnce(
    () =>
      new Promise((_resolve, reject) => {
        fail = reject;
      }),
  );
  const first = loadSessionDetail("a");
  const recovery = loadSessionDetail("a", { showLoading: false });
  const another = loadSessionDetail("a", { showLoading: false });
  fail(new TypeError("Failed to fetch"));
  await Promise.all([first, recovery, another]);
  expect(api.getSession).toHaveBeenCalledTimes(1);
  expect(get(sessionDetail)).toBeNull();

  await loadSessionDetail("a", { showLoading: false });
  expect(api.getSession).toHaveBeenCalledTimes(2);
  expect(get(sessionDetail)?.session.session_id).toBe("a");
  expect(get(sessionDetailError)).toBeNull();
});

test("cancelling an invalidated cached read does not resume fetching the previous Session", async () => {
  await loadSessionDetail("a");
  let release!: (value: SessionView) => void;
  let signal: AbortSignal | null | undefined;
  vi.mocked(api.getSession).mockImplementationOnce((_id, options) => {
    signal = options?.signal;
    return new Promise((resolve) => {
      release = resolve;
    });
  });
  const stale = loadSessionDetail("a");
  await invalidateQueriesForDashboardEvent(sessionEvent("turn.completed"));
  selectSession("b");
  await loadSessionDetail("b");
  release(session("a", "busy"));
  await stale;

  expect(signal?.aborted).toBe(true);
  expect(get(sessionDetail)?.session.session_id).toBe("b");
  expect(api.getSession).toHaveBeenCalledTimes(3);
});

test("completion refreshes an old turns request even after another detail request failed", async () => {
  let releaseTurns!: (value: TurnView[]) => void;
  const running = { turn_id: "turn-1", state: "running" } as TurnView;
  vi.mocked(api.getSession).mockRejectedValueOnce(new TypeError("Failed to fetch"));
  vi.mocked(api.listTurns)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseTurns = resolve;
        }),
    )
    .mockResolvedValue([{ ...running, state: "completed" }]);
  expect(await loadSessionDetail("a")).toBeNull();

  await invalidateQueriesForDashboardEvent(sessionEvent("turn.completed"));
  const terminal = loadSessionDetail("a", { showLoading: false });
  releaseTurns([running]);

  expect((await terminal)?.turns[0].state).toBe("completed");
  expect(get(sessionDetail)?.session.state).toBe("idle");
  expect(get(sessionDetail)?.turns[0].state).toBe("completed");
});

test("a completion refresh waits for a snapshot newer than the in-flight running Turn", async () => {
  vi.useFakeTimers();
  const running = { turn_id: "turn-1", state: "running" } as TurnView;
  vi.mocked(api.getSession).mockResolvedValue(session("a", "busy"));
  vi.mocked(api.listTurns).mockResolvedValue([running]);
  const unsubscribe = sessionDetail.subscribe(() => {});
  const scheduler = createDashboardRefreshScheduler({
    getSelectedSessionId: () => "a",
    getSelectedWorkflowId: () => null,
    getSelectedWorkflowSessionIds: () => [],
    loadWorkflows: async () => {},
    refreshSession: (id) => loadSessionDetail(id, { showLoading: false }),
    refreshWorkflow: async () => {},
  });
  try {
    await loadSessionDetail("a");
    let releaseSession!: (value: SessionView) => void;
    let releaseTurns!: (value: TurnView[]) => void;
    vi.mocked(api.getSession).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseSession = resolve;
        }),
    );
    vi.mocked(api.listTurns).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseTurns = resolve;
        }),
    );
    await invalidateQueriesForDashboardEvent(sessionEvent("turn.started"));
    const older = loadSessionDetail("a");
    vi.mocked(api.getSession).mockResolvedValue(session("a", "idle"));
    vi.mocked(api.listTurns).mockResolvedValue([{ ...running, state: "completed" }]);

    const completed = sessionEvent("turn.completed");
    await invalidateQueriesForDashboardEvent(completed);
    scheduler.handleEvent(completed);
    const terminal = loadSessionDetail("a", { showLoading: false });
    await vi.advanceTimersByTimeAsync(250);
    releaseSession(session("a", "busy"));
    releaseTurns([running]);
    const [, result] = await Promise.all([older, terminal]);

    expect(result?.session.state).toBe("idle");
    expect(result?.turns[0].state).toBe("completed");
    expect(get(sessionDetail)?.turns[0].state).toBe("completed");
  } finally {
    scheduler.reset();
    unsubscribe();
    vi.useRealTimers();
  }
});

test("a later event during the replacement read is not lost", async () => {
  let releaseOlder!: (value: SessionView) => void;
  let releaseReplacement!: (value: SessionView) => void;
  const replacementStarted = new Promise<void>((started) => {
    vi.mocked(api.getSession)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseOlder = resolve;
          }),
      )
      .mockImplementationOnce(() => {
        started();
        return new Promise((resolve) => {
          releaseReplacement = resolve;
        });
      })
      .mockResolvedValue(session("a", "idle"));
  });
  const older = loadSessionDetail("a");
  await invalidateQueriesForDashboardEvent(sessionEvent("turn.started"));
  const started = loadSessionDetail("a", { showLoading: false });
  releaseOlder(session("a", "idle"));
  await replacementStarted;

  await invalidateQueriesForDashboardEvent(sessionEvent("turn.completed"));
  const completed = loadSessionDetail("a", { showLoading: false });
  releaseReplacement(session("a", "busy"));

  for (const result of await Promise.all([older, started, completed])) {
    expect(result?.session.state).toBe("idle");
  }
  expect(get(sessionDetail)?.session.state).toBe("idle");
});

test("an event for another Session does not replace the selected Session's pending read", async () => {
  let release!: (value: SessionView) => void;
  vi.mocked(api.getSession)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    )
    .mockResolvedValue(session("a", "idle"));
  const pending = loadSessionDetail("a");
  await invalidateQueriesForDashboardEvent(sessionEvent("turn.completed", "b"));
  release(session("a", "busy"));

  expect((await pending)?.session.state).toBe("busy");
  expect(get(sessionDetail)?.session.session_id).toBe("a");
});

test("a delayed action refresh for the previous route cannot select it again", async () => {
  selectSession("b");
  await loadSessionDetail("b");
  await loadSessionDetail("a");
  expect(get(selectedSessionId)).toBe("b");
  expect(get(sessionDetail)?.session.session_id).toBe("b");
});
