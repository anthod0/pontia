import { get } from "svelte/store";
import { beforeEach, describe, expect, test, vi } from "vitest";
import type { InboxMessageView, SessionView } from "../../src/api/types";

const baseSession: SessionView = {
  session_id: "session-current",
  client_type: "pi",
  title: "Current session",
  handle: null,
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  workspace: null,
  workspace_id: null,
  workspace_ref: null,
  state: "idle",
  current_turn_id: null,
  state_version: 1,
  metadata: {},
  capabilities: {},
  created_at: "2026-06-22T00:00:00.000Z",
  updated_at: "2026-06-22T00:00:00.000Z",
};

function session(overrides: Partial<SessionView>): SessionView {
  return { ...baseSession, ...overrides };
}

function inboxMessage(overrides: Partial<InboxMessageView> = {}): InboxMessageView {
  return {
    message_id: "message-accepted",
    session_id: "session-current",
    state: "pending",
    delivery_policy: "after_idle",
    input: { summary: "Follow up" },
    metadata: { source: "dashboard_chat" },
    branch_target_turn_id: null,
    turn_id: null,
    steer_target_turn_id: null,
    retry_of_message_id: null,
    retried_by_message_id: null,
    superseded_by_message_id: null,
    failure_message: null,
    created_at: "2026-06-22T00:01:00.000Z",
    updated_at: "2026-06-22T00:01:00.000Z",
    dispatched_at: null,
    cancelled_at: null,
    ...overrides,
  };
}

const api = vi.hoisted(() => ({
  createSession: vi.fn(),
  getSessionOverview: vi.fn(),
  getSession: vi.fn(),
  listTurns: vi.fn(),
  listInboxMessages: vi.fn(),
  listEvents: vi.fn(),
  updateSession: vi.fn(),
  submitInboxMessage: vi.fn(),
  cancelInboxMessage: vi.fn(),
  dismissInboxMessage: vi.fn(),
  interruptSession: vi.fn(),
  resumeSession: vi.fn(),
  terminateSession: vi.fn(),
  pinSession: vi.fn(),
  unpinSession: vi.fn(),
  archiveSession: vi.fn(),
}));

vi.mock("../../src/api/client", () => api);

describe("sessions store management actions", () => {
  beforeEach(() => {
    vi.resetModules();
    Object.values(api).forEach((mock) => mock.mockReset());
    api.getSessionOverview.mockResolvedValue({
      groups: {
        pinned: { sessions: [] },
        active: { sessions: [] },
        list: { sessions: [], next_cursor: null },
      },
    });
  });

  test("creates an optimistic Inbox submission before the request resolves and upgrades it on acceptance", async () => {
    const current = session({ session_id: "session-current" });
    const accepted = inboxMessage();
    let resolveRequest: (message: InboxMessageView) => void = () => undefined;
    api.submitInboxMessage.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveRequest = resolve;
        }),
    );
    api.getSession.mockResolvedValue(current);
    api.listTurns.mockResolvedValue([]);
    api.listInboxMessages.mockResolvedValue([accepted]);
    api.listEvents.mockResolvedValue([]);

    const { selectSession, sessionDetail, submitInboxMessage } =
      await import("../../src/stores/sessions");
    const { setSessionDetail } = await import("../../src/queries/sessions");
    const { optimisticInboxSubmissions } = await import("../../src/queries/inbox");
    selectSession(current.session_id);
    setSessionDetail({ session: current, turns: [], inboxMessages: [], events: [] });

    const submission = submitInboxMessage("session-current", {
      input: "Follow up",
      delivery_policy: "after_idle",
      metadata: { source: "dashboard_chat" },
    });

    expect(optimisticInboxSubmissions("session-current")).toMatchObject([
      { input: "Follow up", showInChat: true, acceptedMessage: null },
    ]);

    await vi.waitFor(() => expect(api.submitInboxMessage).toHaveBeenCalled());
    resolveRequest(accepted);
    await submission;

    expect(optimisticInboxSubmissions("session-current")).toMatchObject([
      { acceptedMessage: { message_id: "message-accepted" } },
    ]);
    expect(get(sessionDetail)?.inboxMessages).toEqual([accepted]);
  });

  test("does not present a busy Session queue submission as a chat message", async () => {
    const busy = session({
      session_id: "session-current",
      state: "busy",
      current_turn_id: "turn-active",
    });
    api.submitInboxMessage.mockImplementation(() => new Promise(() => undefined));

    const { selectSession, submitInboxMessage } = await import("../../src/stores/sessions");
    const { setSessionDetail } = await import("../../src/queries/sessions");
    const { optimisticInboxSubmissions } = await import("../../src/queries/inbox");
    selectSession(busy.session_id);
    setSessionDetail({ session: busy, turns: [], inboxMessages: [], events: [] });

    void submitInboxMessage("session-current", {
      input: "Queue this follow-up",
      delivery_policy: "after_idle",
      metadata: { source: "dashboard_chat" },
    });

    expect(optimisticInboxSubmissions("session-current")).toMatchObject([
      { input: "Queue this follow-up", showInChat: false, acceptedMessage: null },
    ]);
  });

  test.each([
    ["cancel", "cancelInboxMessage", "cancelled"],
    ["dismiss", "dismissInboxMessage", "dismissed"],
  ] as const)("%s updates the cached Inbox message", async (_name, actionName, state) => {
    const updated = inboxMessage({ state });
    api[actionName].mockResolvedValue(updated);
    const actions = await import("../../src/stores/sessions");
    const { queryClient } = await import("../../src/queries/queryClient");
    const { sessionKeys, setSessionDetail } = await import("../../src/queries/sessions");
    setSessionDetail({
      session: session({ session_id: "session-current" }),
      turns: [],
      inboxMessages: [inboxMessage()],
      events: [],
    });

    await actions[actionName]("session-current", "message-accepted");

    expect(queryClient.getQueryData(sessionKeys.inboxMessages("session-current"))).toEqual([
      updated,
    ]);
    expect(
      queryClient.getQueryData(sessionKeys.inboxMessage("session-current", "message-accepted")),
    ).toEqual(updated);
  });

  test("terminating a different session does not replace the current session detail", async () => {
    const current = session({ session_id: "session-current", title: "Current session" });
    const otherExited = session({
      session_id: "session-other",
      title: "Other session",
      state: "exited",
    });
    api.terminateSession.mockResolvedValue(otherExited);
    api.getSession.mockResolvedValue(otherExited);
    api.listTurns.mockResolvedValue([]);
    api.listInboxMessages.mockResolvedValue([]);
    api.listEvents.mockResolvedValue([]);

    const { selectSession, sessionDetail, terminateSession } =
      await import("../../src/stores/sessions");
    const { setSessionDetail } = await import("../../src/queries/sessions");
    selectSession(current.session_id);
    setSessionDetail({ session: current, turns: [], inboxMessages: [], events: [] });

    await terminateSession("session-other");

    let detailSessionId: string | null = null;
    const unsubscribe = sessionDetail.subscribe((detail) => {
      detailSessionId = detail?.session.session_id ?? null;
    });
    unsubscribe();
    expect(detailSessionId).toBe("session-current");
    expect(api.getSession).not.toHaveBeenCalledWith("session-other");
  });
});
