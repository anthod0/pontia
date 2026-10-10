import { render, screen, fireEvent, cleanup } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import QueuedMessages from "../../src/components/chat/QueuedMessages.svelte";
import { visibleChatInboxMessages } from "../../src/components/chat/sessionMetadata";
import type { InboxMessageView } from "../../src/api/types";

function message(state = "pending", overrides: Partial<InboxMessageView> = {}): InboxMessageView {
  return {
    message_id: "message",
    session_id: "session",
    state,
    delivery_policy: "after_idle",
    input: { summary: "continue" },
    metadata: {},
    branch_target_turn_id: null,
    turn_id: null,
    steer_target_turn_id: null,
    retry_of_message_id: null,
    retried_by_message_id: null,
    superseded_by_message_id: null,
    failure_message: null,
    created_at: "",
    updated_at: "",
    dispatched_at: null,
    cancelled_at: null,
    ...overrides,
  };
}

function props(messages: InboxMessageView[]) {
  return {
    sessionId: "session",
    messages,
    busyMessageId: null,
    onCancel: vi.fn(),
    onRetry: vi.fn(),
    onDismiss: vi.fn(),
  };
}

afterEach(cleanup);

test("only pending and failed messages without a replacement are eligible", () => {
  const states = [
    "pending",
    "failed",
    "resuming",
    "dispatching",
    "dispatched",
    "unknown",
    "cancelled",
    "superseded",
    "dismissed",
  ];
  const messages = states.map((state) => message(state, { message_id: state }));
  messages.push(message("pending", { message_id: "replaced", retried_by_message_id: "retry" }));
  expect(visibleChatInboxMessages(messages).map((item) => item.message_id)).toEqual([
    "failed",
    "pending",
  ]);
});

test("pending messages can be cancelled immediately", async () => {
  const input = props([message()]);
  render(QueuedMessages, input);
  await fireEvent.click(screen.getByRole("button", { name: /Cancel inbox message/ }));
  expect(input.onCancel).toHaveBeenCalledWith(message());
});

test("dispatched messages leave the Inbox", async () => {
  const view = render(QueuedMessages, props([message()]));
  await view.rerender({ messages: [message("dispatched")] });
  expect(screen.queryByRole("region")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});

test("failed messages can be retried immediately", async () => {
  const failed = message("failed");
  const input = props([failed]);
  render(QueuedMessages, input);
  await fireEvent.click(screen.getByRole("button", { name: /Retry inbox message/ }));
  expect(input.onRetry).toHaveBeenCalledWith(failed);
});

test("switching sessions excludes stale messages and immediately exposes current actions", async () => {
  const input = props([message()]);
  const view = render(QueuedMessages, input);
  await view.rerender({ sessionId: "other" });
  expect(screen.queryByRole("button")).toBeNull();
  const current = message("pending", { session_id: "other" });
  await view.rerender({ messages: [message(), current] });
  await fireEvent.click(screen.getByRole("button", { name: /Cancel inbox message/ }));
  expect(input.onCancel).toHaveBeenCalledWith(current);
});
