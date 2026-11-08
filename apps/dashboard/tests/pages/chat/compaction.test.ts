import { mocks, session } from "./fixtures";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, expect, test, vi } from "vitest";
import * as api from "../../../src/api/client";
import type { EventView, SessionView } from "../../../src/api/types";
import { chatDraft } from "../../../src/stores/chatDraft";

const SessionChatPage = (await import("../../../src/pages/SessionChatPage.svelte")).default;
afterEach(() => vi.restoreAllMocks());

function renderChat(overrides: Partial<SessionView> = {}, events: EventView[] = []) {
  const selected = session({
    state: "idle",
    ...overrides,
    capabilities: {
      timeline: true,
      manual_compaction: true,
      compaction_history: true,
      ...overrides.capabilities,
    },
  });
  window.history.pushState({}, "", `/dashboard/chat/${selected.session_id}`);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events });
  chatDraft.set("/compact");
  render(SessionChatPage);
  return selected;
}

function event(type: string, operation = "compact-1"): EventView {
  return {
    event_id: `${operation}:${type}`,
    session_id: "session-1",
    turn_id: null,
    source: "pi",
    type: `session.compaction_${type}`,
    time: "2026-11-08T10:00:00Z",
    payload: {
      operation_id: operation,
      trigger: "manual",
      ...(type === "failed"
        ? { aborted: false, error_message: "Provider rejected compaction" }
        : {}),
    },
  };
}

function report(fact: EventView) {
  for (const listener of mocks.dashboardEventListeners)
    listener({ kind: "session_event", id: fact.event_id, occurred_at: fact.time, event: fact });
}

test("manual compaction waits for a terminal fact and refreshes native history without sending a prompt", async () => {
  const compact = vi
    .spyOn(api, "compactSession")
    .mockResolvedValue({ accepted: true, operation_id: "compact-1" });
  renderChat();
  await fireEvent.click(await screen.findByRole("button", { name: "Compact context" }));
  await waitFor(() => expect(get(chatDraft)).toBe(""));
  chatDraft.set("/compact");
  expect(await screen.findByRole("button", { name: "Compact context" })).toBeDisabled();
  report(event("started"));
  await fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
  expect(compact).toHaveBeenCalledExactlyOnceWith("session-1");
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
  report(event("completed"));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Compact context" })).toBeEnabled(),
  );
  expect(mocks.refreshSessionTimeline).toHaveBeenCalledWith("session-1");
  expect(mocks.sessionDetail.get()?.session.state).toBe("idle");
});

test("a completion arriving before the HTTP receipt is not overwritten by the receipt", async () => {
  let accept!: (value: { accepted: boolean; operation_id: string }) => void;
  vi.spyOn(api, "compactSession").mockImplementation(
    () =>
      new Promise((resolve) => {
        accept = resolve;
      }),
  );
  renderChat();
  await fireEvent.click(await screen.findByRole("button", { name: "Compact context" }));
  report(event("completed"));
  report(event("started"));
  accept({ accepted: true, operation_id: "compact-1" });
  await waitFor(() => expect(get(chatDraft)).toBe(""));
  chatDraft.set("/compact");
  expect(await screen.findByRole("button", { name: "Compact context" })).toBeEnabled();
});

test("a rejected request preserves the command and allows a deliberate retry", async () => {
  const compact = vi
    .spyOn(api, "compactSession")
    .mockRejectedValueOnce(new Error("Unavailable"))
    .mockResolvedValue({ accepted: true, operation_id: "compact-2" });
  renderChat();
  await fireEvent.click(await screen.findByRole("button", { name: "Compact context" }));
  await screen.findByRole("alert");
  expect(get(chatDraft)).toBe("/compact");
  await fireEvent.click(screen.getByRole("button", { name: "Compact context" }));
  await waitFor(() => expect(get(chatDraft)).toBe(""));
  expect(compact).toHaveBeenCalledTimes(2);
});

test("a reported failure permits a new request and survives snapshot reload", async () => {
  const compact = vi
    .spyOn(api, "compactSession")
    .mockResolvedValue({ accepted: true, operation_id: "compact-2" });
  renderChat({}, [event("failed"), event("started")]);
  await fireEvent.click(await screen.findByRole("button", { name: "Compact context" }));
  await waitFor(() => expect(compact).toHaveBeenCalledOnce());
  report(event("started", "compact-2"));
  report(event("failed", "compact-2"));
  await screen.findByRole("alert");
  chatDraft.set("/compact");
  expect(await screen.findByRole("button", { name: "Compact context" })).toBeEnabled();
});

test.each([{ state: "busy" }, { state: "exited" }, { capabilities: { manual_compaction: false } }])(
  "unavailable compaction stays local: %j",
  async (overrides) => {
    const compact = vi.spyOn(api, "compactSession");
    renderChat(overrides);
    expect(await screen.findByRole("button", { name: "Compact context" })).toBeDisabled();
    await fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(compact).not.toHaveBeenCalled();
    expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
  },
);

test("compaction history remains readable when ordinary timeline control is unavailable", async () => {
  renderChat({ capabilities: { timeline: false, compaction_history: true } });
  await waitFor(() =>
    expect(mocks.loadSessionTimeline).toHaveBeenCalledWith(
      "session-1",
      expect.objectContaining({ mode: "rebuild" }),
    ),
  );
});
