import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
import type { Bot, SessionView } from "../../src/api/types";
import { openDashboardTab, rememberLastPage, returnToLastPage } from "../../src/lib/lastPage";
import DashboardEntryPage from "../../src/pages/DashboardEntryPage.svelte";

const goto = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("$app/navigation", () => ({ goto }));
vi.mock("../../src/pages/NewChatPage.svelte", async () => ({
  default: (await import("./NewChatPageStub.svelte")).default,
}));

const bot = { bot_id: "pontia" } as Bot;
const notFound = new ApiError("Not found", "not_found", 404);
const savedPage = () => localStorage.getItem("pontia.lastPage:local");

function openTab(path: string) {
  sessionStorage.clear();
  window.history.replaceState({}, "", path);
  openDashboardTab();
}

beforeEach(() => {
  goto.mockClear();
  localStorage.clear();
  testDevice.queryClient.clear();
});
afterEach(() => vi.restoreAllMocks());

test("a new tab on New Chat reopens the last Session chat", async () => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  vi.spyOn(testDevice.api, "getSession").mockResolvedValue({ session_id: "sess-1" } as SessionView);
  openTab("/dashboard/");
  render(DashboardEntryPage);
  expect(screen.getByRole("status", { name: "Loading conversation" })).toHaveAttribute(
    "data-chat-conversation-skeleton",
  );
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/dashboard/chat/sess-1", { replaceState: true }),
  );
});

test("a new tab on a device's New Chat reopens that device's last Bot chat", async () => {
  rememberLastPage("local", { kind: "bot", id: "pontia" });
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot]);
  openTab("/office-mac");
  render(DashboardEntryPage, { routeScope: "/office-mac" });
  expect(screen.getByRole("status", { name: "Loading conversation" })).toHaveAttribute(
    "data-bot-conversation-skeleton",
  );
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/office-mac/bots/pontia", { replaceState: true }),
  );
});

test("a deleted Bot or missing Session opens New Chat and is forgotten", async () => {
  rememberLastPage("local", { kind: "bot", id: "deleted" });
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot]);
  openTab("/dashboard");
  const first = render(DashboardEntryPage);
  expect(await screen.findByText("New Chat")).toBeInTheDocument();
  expect(savedPage()).toBeNull();
  first.unmount();

  rememberLastPage("local", { kind: "chat", id: "gone" });
  vi.spyOn(testDevice.api, "getSession").mockRejectedValue(notFound);
  openTab("/dashboard");
  render(DashboardEntryPage);
  expect(await screen.findByText("New Chat")).toBeInTheDocument();
  expect(savedPage()).toBeNull();
  expect(goto).not.toHaveBeenCalled();
});

test("a Session that fails to load for another reason still opens, to show the error", async () => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  vi.spyOn(testDevice.api, "getSession").mockRejectedValue(new TypeError("Failed to fetch"));
  openTab("/dashboard");
  render(DashboardEntryPage);
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/dashboard/chat/sess-1", { replaceState: true }),
  );
  expect(savedPage()).not.toBeNull();
});

test.each([
  ["a reload", () => window.history.replaceState({}, "", "/dashboard")],
  [
    "a tab opened on another page",
    () => {
      openTab("/dashboard/chat/linked");
      window.history.replaceState({}, "", "/dashboard");
    },
  ],
  ["a New Chat link with a Workspace", () => openTab("/dashboard?workspace=shared")],
])("%s keeps New Chat", async (_name, open) => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  const getSession = vi.spyOn(testDevice.api, "getSession");
  openTab("/dashboard/settings/common");
  open();
  render(DashboardEntryPage);
  expect(screen.getByText("New Chat")).toBeInTheDocument();
  expect(getSession).not.toHaveBeenCalled();
});

test("returning to New Chat later in the same tab keeps New Chat", async () => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  vi.spyOn(testDevice.api, "getSession").mockResolvedValue({ session_id: "sess-1" } as SessionView);
  openTab("/dashboard");
  const first = render(DashboardEntryPage);
  await waitFor(() => expect(goto).toHaveBeenCalled());
  first.unmount();
  render(DashboardEntryPage);
  expect(screen.getByText("New Chat")).toBeInTheDocument();
});

test.each([
  ["Bot chat", { kind: "bot", id: "pontia" }, "/dashboard/bots/pontia"],
  ["Session chat", { kind: "chat", id: "sess-1" }, "/dashboard/chat/sess-1"],
] as const)(
  "Back to chat reopens the last %s after settings was reloaded",
  async (_name, page, path) => {
    rememberLastPage("local", page);
    vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot]);
    vi.spyOn(testDevice.api, "getSession").mockResolvedValue({
      session_id: "sess-1",
    } as SessionView);
    openTab("/dashboard/settings/common");
    // A reload keeps only storage; the tab already showed the Dashboard, so only Back to chat restores.
    sessionStorage.clear();
    sessionStorage.setItem("pontia.tabOpened", "1");
    openDashboardTab();
    returnToLastPage();
    window.history.replaceState({}, "", "/dashboard");
    render(DashboardEntryPage);
    await waitFor(() => expect(goto).toHaveBeenCalledWith(path, { replaceState: true }));
  },
);

test.each([
  ["an exited Session", { session_id: "sess-1", state: "exited", archived_at: null }],
  ["a failed Session", { session_id: "sess-1", state: "error", archived_at: null }],
  ["an archived Session", { session_id: "sess-1", state: "idle", archived_at: "now" }],
])("Back to chat opens New Chat and forgets %s", async (_name, session) => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  vi.spyOn(testDevice.api, "getSession").mockResolvedValue(session as SessionView);
  openTab("/dashboard/settings/common");
  returnToLastPage();
  window.history.replaceState({}, "", "/dashboard");
  render(DashboardEntryPage);
  expect(await screen.findByText("New Chat")).toBeInTheDocument();
  expect(savedPage()).toBeNull();
  expect(goto).not.toHaveBeenCalled();
});

test("Back to chat without a last page opens New Chat", async () => {
  openTab("/dashboard/settings/common");
  returnToLastPage();
  window.history.replaceState({}, "", "/dashboard");
  render(DashboardEntryPage);
  expect(screen.getByText("New Chat")).toBeInTheDocument();
});

test("Back to chat to a device that connects later still reopens its last page", async () => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  vi.spyOn(testDevice.api, "getSession").mockResolvedValue({ session_id: "sess-1" } as SessionView);
  openTab("/dashboard/settings/common");
  returnToLastPage();
  window.history.pushState({}, "", "/dashboard");
  window.dispatchEvent(new PopStateEvent("popstate"));
  render(DashboardEntryPage);
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/dashboard/chat/sess-1", { replaceState: true }),
  );
});

test("a Back to chat request ends when the user navigates elsewhere", async () => {
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  const getSession = vi.spyOn(testDevice.api, "getSession");
  openTab("/dashboard/settings/common");
  returnToLastPage();
  window.history.pushState({}, "", "/dashboard");
  window.dispatchEvent(new PopStateEvent("popstate"));
  window.history.pushState({}, "", "/dashboard/sessions");
  window.dispatchEvent(new PopStateEvent("popstate"));
  window.history.pushState({}, "", "/dashboard");
  window.dispatchEvent(new PopStateEvent("popstate"));
  render(DashboardEntryPage);
  expect(screen.getByText("New Chat")).toBeInTheDocument();
  expect(getSession).not.toHaveBeenCalled();
});

test("a handled Back to chat request does not redirect New Chat when it mounts again", async () => {
  vi.spyOn(testDevice.api, "getSession").mockResolvedValue({
    session_id: "sess-1",
    state: "exited",
    archived_at: null,
  } as SessionView);
  rememberLastPage("local", { kind: "chat", id: "sess-1" });
  openTab("/dashboard/settings/common");
  returnToLastPage();
  window.history.pushState({}, "", "/dashboard");
  window.dispatchEvent(new PopStateEvent("popstate"));
  const first = render(DashboardEntryPage);
  expect(await screen.findByText("New Chat")).toBeInTheDocument();
  first.unmount();
  // Another tab opens a chat; a reconnect mounts New Chat again on the same path.
  rememberLastPage("local", { kind: "chat", id: "sess-2" });
  render(DashboardEntryPage);
  expect(screen.getByText("New Chat")).toBeInTheDocument();
  expect(goto).not.toHaveBeenCalled();
});
