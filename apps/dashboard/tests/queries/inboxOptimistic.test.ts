import { beforeEach, expect, test } from "vitest";
import type { InboxMessageView } from "../../src/api/types";
import type { SessionChatMessage } from "../../src/lib/session-chat/sessionChat";
import {
  consumeInboxSubmission,
  inboxSubmissionMessages,
  optimisticInboxSubmissions,
  reconcileInboxSubmissions,
  syncInboxSubmissions,
  type OptimisticInboxSubmission,
} from "../../src/queries/inbox";
import { queryClient } from "../../src/queries/queryClient";
import { sessionKeys } from "../../src/queries/sessions";

const acceptedInboxMessage = (overrides: Partial<InboxMessageView> = {}): InboxMessageView => ({
  message_id: "message-1",
  session_id: "session-1",
  state: "pending",
  delivery_policy: "after_idle",
  input: { summary: "follow up" },
  metadata: { source: "dashboard_chat" },
  branch_target_turn_id: null,
  turn_id: null,
  steer_target_turn_id: null,
  retry_of_message_id: null,
  retried_by_message_id: null,
  superseded_by_message_id: null,
  failure_message: null,
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  dispatched_at: null,
  cancelled_at: null,
  ...overrides,
});

function submission(overrides: Partial<OptimisticInboxSubmission> = {}): OptimisticInboxSubmission {
  return {
    localId: "message-1",
    sessionId: "session-1",
    input: "follow up",
    deliveryPolicy: "after_idle",
    metadata: { source: "dashboard_chat" },
    branchTargetTurnId: null,
    showInChat: true,
    submittedAt: "2026-05-14T00:00:00Z",
    acceptedMessage: acceptedInboxMessage(),
    ...overrides,
  };
}

function seed(item: OptimisticInboxSubmission): void {
  queryClient.setQueryData(sessionKeys.optimisticInbox(), { "session-1": [item] });
}

beforeEach(() => queryClient.clear());

test("keeps an accepted Inbox submission visible until its projected Turn appears", () => {
  seed(submission());

  expect(inboxSubmissionMessages([], optimisticInboxSubmissions("session-1"))).toMatchObject([
    { id: "optimistic-inbox:message-1:user", content: "follow up", status: "pending" },
  ]);
});

test("removes an optimistic Inbox submission when its projected Turn appears", () => {
  seed(submission({ acceptedMessage: acceptedInboxMessage({ turn_id: "turn-1" }) }));
  const loaded: SessionChatMessage[] = [
    {
      id: "turn-1:user",
      turnId: "turn-1",
      role: "user",
      content: "follow up",
      status: "sent",
      createdAt: "2026-05-14T00:00:00Z",
    },
  ];

  reconcileInboxSubmissions("session-1", loaded);

  expect(optimisticInboxSubmissions("session-1")).toEqual([]);
});

test("removes a submission when SSE reports that its Inbox message was consumed", () => {
  seed(submission({ acceptedMessage: null }));

  consumeInboxSubmission("session-1", "message-1");

  expect(optimisticInboxSubmissions("session-1")).toEqual([]);
});

test("removes a confirmed submission once the Inbox snapshot is terminal", () => {
  seed(submission());

  syncInboxSubmissions("session-1", [acceptedInboxMessage({ state: "dispatched" })]);

  expect(optimisticInboxSubmissions("session-1")).toEqual([]);
});

test("does not invent a chat Turn for queued or branch-targeted submissions", () => {
  const queued = submission({ showInChat: false });
  const branch = submission({ localId: "message-2", branchTargetTurnId: "turn-old" });

  expect(inboxSubmissionMessages([], [queued, branch])).toEqual([]);
});
