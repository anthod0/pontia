import { beforeEach, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import type { InboxMessageView, SessionView } from "../../src/api/types";

const api = vi.hoisted(() => ({
  submitInboxMessage: vi.fn(),
  getSessionOverview: vi.fn(async () => ({
    groups: {
      pinned: { sessions: [] },
      active: { sessions: [] },
      list: { sessions: [], next_cursor: null },
    },
  })),
  getSession: vi.fn(async () => ({ session_id: "session" })),
  listTurns: vi.fn(async () => []),
  listInboxMessages: vi.fn(async () => []),
  listEvents: vi.fn(async () => []),
}));
vi.mock("../../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = await (() => api)(importOriginal);
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});

beforeEach(() => {
  localStorage.clear();
  vi.resetModules();
  vi.clearAllMocks();
});

const pending = (messageId: string) =>
  ({
    message_id: messageId,
    session_id: "session",
    input: { summary: "hello" },
    state: "pending",
    delivery_policy: "after_idle",
    metadata: {},
    branch_target_turn_id: null,
    turn_id: null,
    retried_by_message_id: null,
  }) as unknown as InboxMessageView;

async function seedSession(state: string) {
  const { queryClient } = (await import("../deviceFixture")).testDevice;
  const { sessionKeys } = (await import("../deviceFixture")).testDevice.sessionQueries;
  queryClient.setQueryData(sessionKeys.detail("session"), {
    session_id: "session",
    state,
  } as unknown as SessionView);
}

test("an in-flight submission is persisted but not shown for recovery", async () => {
  const recovery = (await import("../deviceFixture")).testDevice.inboxRecovery;
  const actions = (await import("../deviceFixture")).testDevice.sessions;
  let observedDuringRequest: unknown[] = [];
  let persistedDuringRequest: string | null = null;
  api.submitInboxMessage.mockImplementation(async (_session, _input, id: string) => {
    observedDuringRequest = get(recovery.unconfirmedSubmissions);
    persistedDuringRequest = localStorage.getItem("pontia:inbox-submissions:v2:local");
    return pending(id);
  });

  await actions.submitInboxMessage("session", { input: "hello" });

  expect(observedDuringRequest).toEqual([]);
  expect(persistedDuringRequest).toContain("hello");
  expect(get(recovery.unconfirmedSubmissions)).toEqual([]);
});

test("a pending message sent to an idle Session stays out of the Inbox queue", async () => {
  await seedSession("idle");
  const inbox = (await import("../deviceFixture")).testDevice.inboxQueries;
  const actions = (await import("../deviceFixture")).testDevice.sessions;
  api.submitInboxMessage.mockImplementation(async (_session, _input, id: string) => pending(id));

  const message = await actions.submitInboxMessage("session", {
    input: "hello",
  });

  expect(inbox.queuedInboxMessages([message])).toEqual([]);
  expect(inbox.queuedInboxMessages([{ ...message, state: "failed" }])).toHaveLength(1);
});

test("a message sent to a busy Session is listed in the Inbox queue", async () => {
  await seedSession("busy");
  const inbox = (await import("../deviceFixture")).testDevice.inboxQueries;
  const actions = (await import("../deviceFixture")).testDevice.sessions;
  api.submitInboxMessage.mockImplementation(async (_session, _input, id: string) => pending(id));

  const message = await actions.submitInboxMessage("session", {
    input: "hello",
  });

  expect(inbox.queuedInboxMessages([message])).toEqual([message]);
});

test.each(["busy", "waiting_for_interaction"])(
  "a submission to a %s Session stays in Inbox until delivery",
  async (state) => {
    await seedSession(state);
    const device = (await import("../deviceFixture")).testDevice;
    let accept!: () => void;
    api.submitInboxMessage.mockImplementation(
      (_session, _input, id: string) =>
        new Promise((resolve) => {
          accept = () => resolve(pending(id));
        }),
    );

    const submitting = device.sessions.submitInboxMessage("session", { input: "hello" });
    const submissions = () => device.inboxQueries.optimisticInboxSubmissions("session");
    const chatMessages = () => device.inboxQueries.inboxSubmissionMessages([], submissions());
    await vi.waitFor(() =>
      expect(device.inboxQueries.queuedInboxMessages([], submissions())).toMatchObject([
        { input: { summary: "hello" }, state: "pending" },
      ]),
    );
    expect(chatMessages()).toEqual([]);
    accept();
    const message = await submitting;
    expect(device.inboxQueries.queuedInboxMessages([message], submissions())).toEqual([message]);
    expect(chatMessages()).toEqual([]);

    device.inboxQueries.syncInboxSubmissions("session", [{ ...message, state: "dispatched" }]);
    expect(device.inboxQueries.queuedInboxMessages([message], submissions())).toEqual([]);
    expect(chatMessages()).toMatchObject([{ role: "user", content: "hello" }]);
  },
);

test("a delayed Inbox snapshot cannot overwrite an acknowledged submission", async () => {
  await seedSession("busy");
  const device = (await import("../deviceFixture")).testDevice;
  device.sessionQueries.setSessionDetail({
    session: { session_id: "session", state: "busy" } as SessionView,
    turns: [],
    inboxMessages: [],
    events: [],
  });
  device.sessions.selectSession("session");
  const unsubscribe = device.sessions.sessionDetail.subscribe(() => {});
  let finishOldSnapshot!: (messages: InboxMessageView[]) => void;
  api.listInboxMessages.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishOldSnapshot = resolve;
      }),
  );
  api.submitInboxMessage.mockImplementation(async (_session, _input, id: string) => pending(id));

  try {
    const loading = device.sessions.loadSessionDetail("session");
    const message = await device.sessions.submitInboxMessage("session", { input: "hello" });
    expect(get(device.sessions.sessionDetail)?.inboxMessages).toEqual([message]);
    api.listInboxMessages.mockResolvedValueOnce([message]);
    finishOldSnapshot([]);
    await loading;

    expect(get(device.sessions.sessionDetail)?.inboxMessages).toEqual([message]);
  } finally {
    unsubscribe();
    device.sessions.selectSession(null);
  }
});
