import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SessionInteractions from "../src/components/chat/SessionInteractions.svelte";
import type { DashboardStreamEvent, Interaction } from "../src/api/types";
import { ApiError } from "../src/api/errors";
import { sseStatus } from "../src/stores/connection";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  respond: vi.fn(),
  get: vi.fn(),
  toastError: vi.fn(),
  listener: null as null | ((event: DashboardStreamEvent) => void),
}));
vi.mock("svelte-sonner", () => ({
  toast: { error: mocks.toastError },
}));
vi.mock("../src/api/client", () => ({
  listPendingInteractions: mocks.list,
  respondInteraction: mocks.respond,
  getInteraction: mocks.get,
}));
vi.mock("../src/services/eventStream", () => ({
  subscribeDashboardEvents: (listener: (event: DashboardStreamEvent) => void) => {
    mocks.listener = listener;
    return () => {
      mocks.listener = null;
    };
  },
}));

function interaction(): Interaction {
  return {
    interaction_id: "choice-1",
    session_id: "session-1",
    turn_id: null,
    type: "choice",
    state: "pending",
    title: "<img src=x onerror=alert(1)>",
    description: "<script>bad()</script>",
    options: [
      { key: "execute-once", label: "Proceed now", intent: "positive" },
      { key: "skip-operation", label: "Skip", intent: "negative" },
    ],
    response: null,
    requested_at: "2026-10-13T00:00:00Z",
    resolved_at: null,
  };
}
beforeEach(() => {
  vi.clearAllMocks();
  sseStatus.set("closed");
  mocks.list.mockResolvedValue([interaction()]);
});
afterEach(() => {
  sseStatus.set("closed");
});

test("renders untrusted text safely and submits only the selected arbitrary option key once", async () => {
  let resolve!: (value: Interaction) => void;
  mocks.respond.mockReturnValue(
    new Promise<Interaction>((done) => {
      resolve = done;
    }),
  );
  const { container } = render(SessionInteractions, { sessionId: "session-1" });
  const button = await screen.findByRole("button", { name: "Proceed now" });
  expect(container.querySelector("img, script")).toBeNull();
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  expect(mocks.respond).toHaveBeenCalledExactlyOnceWith("session-1", "choice-1", "execute-once");
  resolve({ ...interaction(), state: "resolved", response: { option_key: "execute-once" } });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Proceed now" })).toBeNull());
});

test("a conflicting response re-reads the authoritative interaction and disables further choices", async () => {
  mocks.respond.mockRejectedValue(new ApiError("Conflict", "interaction_expired", 409));
  mocks.get.mockResolvedValue({ ...interaction(), state: "expired" });
  render(SessionInteractions, { sessionId: "session-1" });
  await fireEvent.click(await screen.findByRole("button", { name: "Skip" }));
  await waitFor(() => expect(mocks.get).toHaveBeenCalledWith("session-1", "choice-1"));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Skip" })).toBeNull());
});

test("refreshes pending choices on notifications, reconnect and page visibility restoration", async () => {
  render(SessionInteractions, { sessionId: "session-1" });
  await screen.findByRole("button", { name: "Skip" });
  mocks.list.mockResolvedValue([]);
  mocks.listener?.({
    kind: "interaction_changed",
    session_id: "session-1",
    interaction_id: "choice-1",
  });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Skip" })).toBeNull());
  mocks.list.mockResolvedValue([interaction()]);
  sseStatus.set("open");
  await screen.findByRole("button", { name: "Skip" });
  mocks.list.mockResolvedValue([]);
  document.dispatchEvent(new Event("visibilitychange"));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Skip" })).toBeNull());
});

test("notifies once for repeated refresh failures and recovers pending choices", async () => {
  mocks.list.mockRejectedValue(new ApiError("Not Found", "request_failed", 404));
  render(SessionInteractions, { sessionId: "session-1" });
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));

  sseStatus.set("open");
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
  expect(mocks.toastError).toHaveBeenCalledTimes(1);

  mocks.list.mockResolvedValue([interaction()]);
  document.dispatchEvent(new Event("visibilitychange"));
  await screen.findByRole("button", { name: "Skip" });

  mocks.list.mockRejectedValue(new ApiError("Not Found", "request_failed", 404));
  document.dispatchEvent(new Event("visibilitychange"));
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(2));
});

test("notifies on a failed response and lets the user retry the pending choice", async () => {
  mocks.respond.mockRejectedValueOnce(new Error("Response failed"));
  render(SessionInteractions, { sessionId: "session-1" });
  await fireEvent.click(await screen.findByRole("button", { name: "Skip" }));
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
  expect(screen.getByRole("button", { name: "Skip" })).toBeEnabled();

  mocks.respond.mockResolvedValue({ ...interaction(), state: "resolved", response: { option_key: "skip-operation" } });
  await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Skip" })).toBeNull());
});

test("keeps the authoritative conflict outcome when pending refresh races with its read", async () => {
  let resolve!: (value: Interaction) => void;
  mocks.respond.mockRejectedValue(new ApiError("Conflict", "interaction_expired", 409));
  mocks.get.mockReturnValue(
    new Promise<Interaction>((done) => {
      resolve = done;
    }),
  );
  render(SessionInteractions, { sessionId: "session-1" });
  await fireEvent.click(await screen.findByRole("button", { name: "Skip" }));
  await waitFor(() => expect(mocks.get).toHaveBeenCalledWith("session-1", "choice-1"));
  mocks.list.mockResolvedValue([]);
  mocks.listener?.({
    kind: "interaction_changed",
    session_id: "session-1",
    interaction_id: "choice-1",
  });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Skip" })).toBeNull());
  resolve({ ...interaction(), state: "expired" });
  await screen.findByRole("status");
  mocks.listener?.({
    kind: "interaction_changed",
    session_id: "session-1",
    interaction_id: "choice-1",
  });
  await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
  expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
});
