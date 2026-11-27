import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import QueuedMessages from "../../src/components/chat/QueuedMessages.svelte";
import type { QueuedMessageView } from "../../src/api/types";

function message(overrides: Partial<QueuedMessageView> = {}): QueuedMessageView {
  return {
    message_id: "message",
    session_id: "session",
    input: "continue",
    metadata: {},
    created_at: "2026-11-28T00:00:00Z",
    ...overrides,
  };
}

function props(messages: QueuedMessageView[]) {
  return {
    sessionId: "session",
    messages,
    busyMessageId: null,
    onCancel: vi.fn(),
  };
}

afterEach(cleanup);

test("queued messages can be cancelled", async () => {
  const input = props([message()]);
  render(QueuedMessages, input);
  await fireEvent.click(screen.getByRole("button", { name: /Cancel queued message/ }));
  expect(input.onCancel).toHaveBeenCalledWith(message());
});

test("switching sessions hides stale queue entries", async () => {
  const input = props([message()]);
  const view = render(QueuedMessages, input);
  await view.rerender({ sessionId: "other" });
  expect(screen.queryByRole("button")).toBeNull();
});

test("only entries for the selected session are actionable", async () => {
  const current = message({ message_id: "current", session_id: "other" });
  const input = props([message(), current]);
  render(QueuedMessages, { ...input, sessionId: "other" });
  await fireEvent.click(screen.getByRole("button", { name: /Cancel queued message/ }));
  expect(input.onCancel).toHaveBeenCalledWith(current);
});
