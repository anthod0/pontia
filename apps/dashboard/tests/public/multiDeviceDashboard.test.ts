import { File } from "node:buffer";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import MultiDeviceDashboardHarness from "../components/MultiDeviceDashboardHarness.svelte";
import { createRemoteAccessFixture } from "./fixtures/remoteConnections";
import {
  saveUploadRecord,
  loadUploadRecords,
  removeUploadRecord,
} from "../../src/lib/uploads/records";

let fixture: ReturnType<typeof createRemoteAccessFixture>;
const goto = vi.hoisted(() => vi.fn(async (url: string) => window.history.pushState({}, "", url)));
vi.mock("$app/navigation", () => ({ goto }));
vi.mock("../../src/modes/public/remote-access", async () => ({
  ...(await import("../../src/modes/public/remote-access/remoteAccess")),
  listPublicDevices: (signal: AbortSignal) => fixture.remote.listPublicDevices(signal),
  createRemoteConnection: (handle: string, onFailure: (error: unknown) => void) =>
    fixture.remote.createRemoteConnection(handle, onFailure),
}));

const workspace = (id: string) => ({
  workspace_id: id,
  name: id,
  canonical_path: `/repo/${id}`,
  display_path: `/repo/${id}`,
  state: "active",
  metadata: {},
  created_at: "now",
  updated_at: "now",
  last_used_at: null,
});
const session = (handle: string, id = "same") => ({
  session_id: id,
  title: `${handle} chat`,
  handle: id,
  client_type: "pi",
  state: "idle",
  workspace_id: "shared",
  capabilities: {},
  metadata: {},
  created_at: "now",
  updated_at: "now",
});
const response = (data: unknown) => new Response(JSON.stringify({ data }));

beforeEach(async () => {
  for (const handle of ["office-mac", "travel-laptop"]) {
    for (const record of await loadUploadRecords(handle, "new:shared"))
      await removeUploadRecord(record);
  }
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.releasePointerCapture = () => {};
  localStorage.clear();
  goto.mockClear();
  window.history.replaceState({}, "", "/");
  fixture = createRemoteAccessFixture();
  const baseRequest = fixture.request.getMockImplementation()!;
  fixture.request.mockImplementation(async (handle, path, init) => {
    const url = new URL(path, "http://device");
    if (url.pathname.endsWith("/workspaces"))
      return response({ workspaces: [workspace("shared"), workspace("other")] });
    if (url.pathname.endsWith("/sessions/overview")) {
      const sessions = url.searchParams.get("workspace_id") === "other" ? [] : [session(handle)];
      return new Response(
        JSON.stringify({
          groups: {
            pinned: { sessions: [] },
            active: { sessions: [session(handle)] },
            list: { sessions, next_cursor: null },
          },
        }),
      );
    }
    if (url.pathname.endsWith("/session-creation-options"))
      return response({
        options: [
          {
            id: handle,
            name: `${handle} agent`,
            client_type: handle === "office-mac" ? "pi" : "codex",
            metadata: { device_option: handle },
          },
        ],
      });
    if (url.pathname.endsWith("/sessions") && init.method === "POST")
      return response({ session: session(handle, "created"), initial_turn: null });
    if (url.pathname.endsWith("/sessions/same/pin") && init.method === "POST")
      return response({ session: session(handle) });
    if (url.pathname.includes("/uploads/"))
      return response({
        upload: {
          upload_id: "file",
          name: "report.txt",
          size: 3,
          sha256: "a".repeat(64),
          offset: 3,
          completed: true,
          path: `/${handle}/report.txt`,
          created_at: "now",
        },
      });
    if (url.pathname.endsWith("/sessions/created"))
      return response({ session: session(handle, "created"), turns: [] });
    if (url.pathname.includes("/turns/timeline"))
      return response({ items: [], groups: [], has_more: false });
    if (url.pathname.endsWith("/turns")) return response({ turns: [] });
    if (url.pathname.endsWith("/inbox-messages")) return response({ messages: [] });
    if (url.pathname.endsWith("/events")) return response({ events: [] });
    return baseRequest(handle, path, init);
  });
});

async function chooseWorkspace(name: string) {
  await userEvent.click(await screen.findByLabelText("Workspace", { exact: true }));
  await userEvent.click(await screen.findByText(name, { exact: true }));
  document.body.style.pointerEvents = "";
}

test("sidebar actions, filters, and same-ID highlighting stay with their device across SPA navigation", async () => {
  window.history.replaceState({}, "", "/office-mac/chat/same");
  render(MultiDeviceDashboardHarness);
  const office = await screen.findByRole("region", { name: "Office Mac sessions" });
  const travel = await screen.findByRole("region", { name: "Travel Laptop sessions" });
  const officeChat = await within(office).findByRole("button", { name: "office-mac chat" });
  const travelChat = await within(travel).findByRole("button", { name: "travel-laptop chat" });
  expect(officeChat).toHaveAttribute("data-active", "true");
  expect(travelChat).not.toHaveAttribute("data-active");
  await userEvent.click(travelChat);
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/same"));
  expect(travelChat).toHaveAttribute("data-active", "true");
  expect(officeChat).not.toHaveAttribute("data-active");
  await userEvent.click(
    within(office).getByRole("button", { name: "Open session actions for office-mac chat" }),
  );
  await userEvent.click(await screen.findByText("Pin", { exact: true }));
  await waitFor(() =>
    expect(
      fixture.request.mock.calls.some(
        ([handle, path, init]) =>
          handle === "office-mac" && path.endsWith("/sessions/same/pin") && init.method === "POST",
      ),
    ).toBe(true),
  );
  document.body.style.pointerEvents = "";
  await userEvent.click(
    within(office).getByRole("button", { name: "Filter recent sessions by workspace" }),
  );
  await userEvent.click(await screen.findByText("other", { exact: true }));
  await waitFor(() =>
    expect(
      within(office).queryByRole("button", { name: "office-mac chat" }),
    ).not.toBeInTheDocument(),
  );
  expect(within(travel).getByRole("button", { name: "travel-laptop chat" })).toBeInTheDocument();
  document.body.style.pointerEvents = "";
  await userEvent.click(within(office).getByRole("link", { name: "All sessions on Office Mac" }));
  await waitFor(() => expect(location.pathname).toBe("/office-mac/sessions"));
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("root creation uses the selected device options and restores its draft and attachments", async () => {
  await saveUploadRecord({
    scope: "office-mac",
    draft: "new:shared",
    key: "root-file",
    uploadId: "file",
    name: "report.txt",
    size: 3,
    sha256: "a".repeat(64),
  });
  render(MultiDeviceDashboardHarness);
  await userEvent.type(await screen.findByRole("textbox"), "Office task");
  await screen.findByRole("button", { name: "Remove attachment report.txt" });
  await chooseWorkspace("shared · Travel Laptop");
  expect(screen.getByRole("textbox")).toHaveTextContent("");
  expect(
    screen.queryByRole("button", { name: "Remove attachment report.txt" }),
  ).not.toBeInTheDocument();
  await userEvent.type(screen.getByRole("textbox"), "Travel task");
  await chooseWorkspace("shared · Office Mac");
  expect(screen.getByRole("textbox")).toHaveTextContent("Office task");
  await screen.findByRole("button", { name: "Remove attachment report.txt" });
  await chooseWorkspace("shared · Travel Laptop");
  expect(screen.getByRole("textbox")).toHaveTextContent("Travel task");
  await userEvent.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/created"));
  const submissions = fixture.request.mock.calls.filter(
    ([, path, init]) => path.endsWith("/sessions") && init.method === "POST",
  );
  expect(submissions).toHaveLength(1);
  expect(submissions[0][0]).toBe("travel-laptop");
  expect(JSON.parse(submissions[0][2].body as string)).toMatchObject({
    client_type: "codex",
    workspace_id: "shared",
    initial_task: { input: "Travel task" },
    metadata: { device_option: "travel-laptop" },
  });
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("root honors device-qualified workspace query parameters ahead of remembered selection", async () => {
  localStorage.setItem("pontia.chat.lastDeviceWorkspace", JSON.stringify(["office-mac", "shared"]));
  window.history.replaceState({}, "", "/?device=travel-laptop&workspace=other");
  render(MultiDeviceDashboardHarness);
  await userEvent.type(await screen.findByRole("textbox"), "Use query workspace");
  await userEvent.click(await screen.findByRole("button", { name: "Start session" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/created"));
  const call = fixture.request.mock.calls.find(
    ([, path, init]) => path.endsWith("/sessions") && init.method === "POST",
  )!;
  expect(JSON.parse(call[2].body as string).workspace_id).toBe("other");
});

test("device new chat stays scoped, and New Chat opens the global root without reconnecting", async () => {
  window.history.replaceState({}, "", "/office-mac");
  render(MultiDeviceDashboardHarness);
  await userEvent.click(await screen.findByLabelText("Workspace", { exact: true }));
  await userEvent.click(await screen.findByText("other", { exact: true }));
  document.body.style.pointerEvents = "";
  await userEvent.click(screen.getByRole("button", { name: /New Chat/ }));
  await waitFor(() => expect(location.pathname).toBe("/"));
  await chooseWorkspace("shared · Travel Laptop");
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("settings selection preserves section and permits an offline target and recovery", async () => {
  window.history.replaceState({}, "", "/office-mac/settings/clients");
  render(MultiDeviceDashboardHarness);
  await screen.findByText("Device page");
  fixture.connectionFailures.get("travel-laptop")!(new Error("offline"));
  await userEvent.click(screen.getByLabelText("Settings device"));
  await userEvent.click(await screen.findByText("Travel Laptop · Offline", { exact: true }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/clients"));
  expect(screen.getByLabelText("Settings device")).toBeEnabled();
  document.body.style.pointerEvents = "";
  await userEvent.click(await screen.findByRole("button", { name: "Try again" }));
  await screen.findByText("Device page");
  expect(fixture.signals.get("office-mac")).toHaveLength(1);
});

test("root settings uses the last device, with the first online device as fallback", async () => {
  localStorage.setItem("pontia.lastDeviceHandle", "travel-laptop");
  // Keep the root in its empty state so opening a workspace does not change last use.
  const request = fixture.request.getMockImplementation()!;
  fixture.request.mockImplementation((handle, path, init) =>
    path.endsWith("/workspaces")
      ? Promise.resolve(response({ workspaces: [] }))
      : request(handle, path, init),
  );
  const view = render(MultiDeviceDashboardHarness);
  await waitFor(() => expect(fixture.streams.size).toBe(2));
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/common"));
  view.unmount();
  localStorage.clear();
  window.history.replaceState({}, "", "/");
  render(MultiDeviceDashboardHarness);
  await waitFor(() => expect(screen.getByRole("button", { name: "Settings" })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(location.pathname).toBe("/office-mac/settings/common"));
});

test("without an online or remembered device settings stays disabled and offline devices can retry", async () => {
  fixture.connect.mockRejectedValue(new Error("offline"));
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("button", { name: "Retry Office Mac" });
  expect(screen.getByRole("button", { name: "Settings" })).toBeDisabled();
  fixture.connect.mockImplementation(async (handle) => fixture.target(handle));
  await userEvent.click(screen.getByRole("button", { name: "Retry Office Mac" }));
  await screen.findByRole("textbox");
  expect(screen.getByRole("button", { name: "Settings" })).toBeEnabled();
});

test("session shortcuts on the root use the selected device", async () => {
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  await chooseWorkspace("shared · Travel Laptop");
  await fireEvent.keyDown(window, { key: "1", code: "Digit1", altKey: true });
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/same"));
});

test("root automatic workspace selection does not replace the last settings device", async () => {
  window.history.replaceState({}, "", "/travel-laptop/chat/same");
  render(MultiDeviceDashboardHarness);
  await screen.findByText("Device page");
  await userEvent.click(screen.getByRole("button", { name: /New Chat/ }));
  await screen.findByRole("textbox");
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/common"));
});

test("an attachment still hashing survives device switches and uploads only to its owner", async () => {
  let finishHash!: (bytes: ArrayBuffer) => void;
  const bytes = new Promise<ArrayBuffer>((resolve) => {
    finishHash = resolve;
  });
  const file = new File(["abc"], "pending.txt");
  vi.spyOn(file, "slice").mockReturnValue({ arrayBuffer: () => bytes } as Blob);
  const request = fixture.request.getMockImplementation()!;
  fixture.request.mockImplementation((handle, path, init) => {
    if (path.includes("/uploads"))
      return Promise.resolve(
        response({
          upload: {
            upload_id: "pending",
            name: "pending.txt",
            size: 3,
            sha256: "a".repeat(64),
            offset: 3,
            completed: true,
            path: `/${handle}/pending.txt`,
            created_at: "now",
          },
        }),
      );
    return request(handle, path, init);
  });
  render(MultiDeviceDashboardHarness);
  await waitFor(() => expect(screen.getByRole("button", { name: "Upload files" })).toBeEnabled());
  await fireEvent.change(screen.getByLabelText("Select files to upload"), {
    target: { files: [file] },
  });
  await screen.findByRole("button", { name: "Remove attachment pending.txt" });
  await chooseWorkspace("shared · Travel Laptop");
  expect(
    screen.queryByRole("button", { name: "Remove attachment pending.txt" }),
  ).not.toBeInTheDocument();
  await chooseWorkspace("shared · Office Mac");
  await screen.findByRole("button", { name: "Remove attachment pending.txt" });
  finishHash(new TextEncoder().encode("abc").buffer);
  await waitFor(() => expect(screen.getByRole("button", { name: "Start session" })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() => expect(location.pathname).toBe("/office-mac/chat/created"));
  const uploads = fixture.request.mock.calls.filter(([, path]) => path.includes("/uploads"));
  expect(uploads.length).toBeGreaterThan(0);
  expect(uploads.every(([handle]) => handle === "office-mac")).toBe(true);
  const submission = fixture.request.mock.calls.find(
    ([, path, init]) => path.endsWith("/sessions") && init.method === "POST",
  )!;
  expect(JSON.parse(submission[2].body as string).initial_task.input).toBe(
    '<user_upload path="/office-mac/pending.txt">pending.txt</user_upload>',
  );
});

test("root login expiry offers sign-in and cannot restart device connections", async () => {
  vi.mocked(fixture.remote.listPublicDevices).mockRejectedValueOnce(
    new fixture.remote.CloudRequestError("unauthorized", 401),
  );
  render(MultiDeviceDashboardHarness);
  const signIn = await screen.findByRole("link", { name: "Sign in" });
  expect(signIn.getAttribute("href")).toContain("https://pontia.dev/login?");
  expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  await fireEvent.focus(window);
  expect(fixture.connect).not.toHaveBeenCalled();
  expect(fixture.remote.listPublicDevices).toHaveBeenCalledTimes(1);
});

test("returning to root restores the remembered device Workspace even with cached lists", async () => {
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  await chooseWorkspace("other · Travel Laptop");
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/common"));
  await userEvent.click(screen.getByRole("button", { name: /New Chat/ }));
  await userEvent.type(await screen.findByRole("textbox"), "Remember this workspace");
  await userEvent.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/created"));
  const submission = fixture.request.mock.calls.find(
    ([, path, init]) => path.endsWith("/sessions") && init.method === "POST",
  )!;
  expect(JSON.parse(submission[2].body as string).workspace_id).toBe("other");
});
