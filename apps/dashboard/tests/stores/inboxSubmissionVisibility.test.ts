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
vi.mock("../../src/api/client", () => api);

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
  const { queryClient } = await import("../../src/queries/queryClient");
  const { sessionKeys } = await import("../../src/queries/sessions");
  queryClient.setQueryData(sessionKeys.detail("session"), {
    session_id: "session",
    state,
  } as unknown as SessionView);
}

test("an in-flight submission is persisted but not shown for recovery", async () => {
  const recovery = await import("../../src/stores/inboxRecovery");
  const actions = await import("../../src/stores/sessions");
  let observedDuringRequest: unknown[] = [];
  let persistedDuringRequest: string | null = null;
  api.submitInboxMessage.mockImplementation(
    async (_session, _input, id: string) => {
      observedDuringRequest = get(recovery.unconfirmedSubmissions);
      persistedDuringRequest = localStorage.getItem(
        "pontia:inbox-submissions:v1",
      );
      return pending(id);
    },
  );

  await actions.submitInboxMessage("session", { input: "hello" });

  expect(observedDuringRequest).toEqual([]);
  expect(persistedDuringRequest).toContain("hello");
  expect(get(recovery.unconfirmedSubmissions)).toEqual([]);
});

test("a pending message sent to an idle Session stays out of the Inbox queue", async () => {
  await seedSession("idle");
  const inbox = await import("../../src/queries/inbox");
  const actions = await import("../../src/stores/sessions");
  api.submitInboxMessage.mockImplementation(
    async (_session, _input, id: string) => pending(id),
  );

  const message = await actions.submitInboxMessage("session", {
    input: "hello",
  });

  expect(inbox.queuedInboxMessages([message])).toEqual([]);
  expect(
    inbox.queuedInboxMessages([{ ...message, state: "failed" }]),
  ).toHaveLength(1);
});

test("a message sent to a busy Session is listed in the Inbox queue", async () => {
  await seedSession("busy");
  const inbox = await import("../../src/queries/inbox");
  const actions = await import("../../src/stores/sessions");
  api.submitInboxMessage.mockImplementation(
    async (_session, _input, id: string) => pending(id),
  );

  const message = await actions.submitInboxMessage("session", {
    input: "hello",
  });

  expect(inbox.queuedInboxMessages([message])).toEqual([message]);
});
