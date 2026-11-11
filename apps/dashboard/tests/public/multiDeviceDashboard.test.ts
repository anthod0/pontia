import { File } from "node:buffer";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
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
const redirectToSignIn = vi.hoisted(() => vi.fn());
vi.mock("../../src/modes/public/signInRedirect", () => ({ redirectToSignIn }));
vi.mock("../../src/modes/public/remote-access", async () => ({
  ...(await import("../../src/modes/public/remote-access/remoteAccess")),
  listPublicDevices: (signal: AbortSignal) => fixture.remote.listPublicDevices(signal),
  createRemoteConnection: (handle: string, onFailure: (error: unknown) => void) =>
    fixture.remote.createRemoteConnection(handle, onFailure),
}));

const workspace = (id: string) => ({
  availability: { status: "available", root_id: "projects", path: id },
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
  redirectToSignIn.mockClear();
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

const sidebar = () => within(document.querySelector<HTMLElement>('[data-sidebar="content"]')!);

async function chooseSidebarDevice(name: string) {
  await userEvent.click(screen.getByRole("button", { name: "Sidebar device" }));
  const items = await screen.findAllByRole("menuitemradio", { hidden: true });
  await userEvent.click(items.find((item) => item.textContent?.includes(name))!);
  document.body.style.pointerEvents = "";
}

test("sidebar device switcher scopes the session list, actions, filters, and highlighting", async () => {
  window.history.replaceState({}, "", "/office-mac/chat/same");
  render(MultiDeviceDashboardHarness);
  expect(await screen.findByRole("button", { name: "Sidebar device" })).toHaveTextContent(
    "Office Mac",
  );
  const officeChat = await sidebar().findByRole("button", { name: "office-mac chat" });
  expect(officeChat).toHaveAttribute("data-active", "true");
  expect(sidebar().queryByRole("button", { name: "travel-laptop chat" })).not.toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Open session actions for office-mac chat" }),
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
    screen.getByRole("button", { name: "Filter recent sessions by workspace" }),
  );
  await userEvent.click(await screen.findByText("other", { exact: true }));
  await waitFor(() =>
    expect(sidebar().queryByRole("button", { name: "office-mac chat" })).not.toBeInTheDocument(),
  );
  document.body.style.pointerEvents = "";

  await chooseSidebarDevice("Travel Laptop");
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop"));
  const travelChat = await sidebar().findByRole("button", { name: "travel-laptop chat" });
  expect(travelChat).not.toHaveAttribute("data-active");
  await userEvent.click(travelChat);
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/same"));
  expect(travelChat).toHaveAttribute("data-active", "true");
  await userEvent.click(screen.getByRole("button", { name: "Open all sessions" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/sessions"));
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("sidebar follows the routed device after navigation", async () => {
  window.history.replaceState({}, "", "/office-mac/chat/same");
  const view = render(MultiDeviceDashboardHarness);
  await sidebar().findByRole("button", { name: "office-mac chat" });
  for (const [path, chat, name] of [
    ["/travel-laptop/sessions", "travel-laptop chat", "Travel Laptop"],
    ["/office-mac/sessions", "office-mac chat", "Office Mac"],
  ]) {
    window.history.pushState({}, "", path);
    window.dispatchEvent(new PopStateEvent("popstate"));
    await sidebar().findByRole("button", { name: chat });
    expect(screen.getByRole("button", { name: "Sidebar device" })).toHaveTextContent(name);
  }
  view.unmount();
});

test("root uses the last routed device, with the first listed device as fallback", async () => {
  render(MultiDeviceDashboardHarness);
  expect(await screen.findByRole("button", { name: "Sidebar device" })).toHaveTextContent(
    "Office Mac",
  );
  await sidebar().findByRole("button", { name: "office-mac chat" });
  cleanup();
  localStorage.setItem("pontia.lastDeviceHandle", "travel-laptop");
  render(MultiDeviceDashboardHarness);
  expect(await screen.findByRole("button", { name: "Sidebar device" })).toHaveTextContent(
    "Travel Laptop",
  );
  await sidebar().findByRole("button", { name: "travel-laptop chat" });
  await userEvent.type(await screen.findByRole("textbox"), "Travel task");
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
});

test("root workspace selector lists only the active device workspaces", async () => {
  render(MultiDeviceDashboardHarness);
  await userEvent.click(await screen.findByLabelText("Workspace", { exact: true }));
  const options = await screen.findAllByRole("option", { hidden: true });
  expect(options.map((option) => option.textContent)).toEqual([
    expect.stringContaining("shared"),
    expect.stringContaining("other"),
  ]);
  expect(options.some((option) => option.textContent?.includes("Laptop"))).toBe(false);
});

test("a single device hides the sidebar and composer device selectors", async () => {
  vi.mocked(fixture.remote.listPublicDevices).mockResolvedValue([
    { handle: "office-mac", name: "Office Mac" },
  ]);
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  expect(await sidebar().findByRole("button", { name: "office-mac chat" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Sidebar device" })).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Device", { exact: true })).not.toBeInTheDocument();
});

test("choosing a sidebar device on the root opens that device", async () => {
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  await chooseSidebarDevice("Travel Laptop");
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop"));
  window.history.pushState({}, "", "/travel-laptop/chat/same");
  window.dispatchEvent(new PopStateEvent("popstate"));
  await userEvent.click(screen.getByRole("button", { name: /New Chat/ }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop"));
  expect(screen.getByRole("button", { name: "Sidebar device" })).toHaveTextContent("Travel Laptop");
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("the root composer device selector switches devices like the sidebar", async () => {
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  const select = screen.getByLabelText("Device", { exact: true });
  expect(select).toHaveTextContent("Office Mac");
  await userEvent.click(select);
  const options = await screen.findAllByRole("option", { hidden: true });
  await userEvent.click(options.find((option) => option.textContent?.includes("Travel Laptop"))!);
  document.body.style.pointerEvents = "";
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop"));
  expect(screen.queryByLabelText("Device", { exact: true })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Sidebar device" })).toHaveTextContent("Travel Laptop");
});

test("root honors the workspace query parameter for the active device", async () => {
  localStorage.setItem("pontia.lastDeviceHandle", "travel-laptop");
  window.history.replaceState({}, "", "/?workspace=other");
  render(MultiDeviceDashboardHarness);
  await userEvent.type(await screen.findByRole("textbox"), "Use query workspace");
  await userEvent.click(await screen.findByRole("button", { name: "Start session" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/created"));
  const call = fixture.request.mock.calls.find(
    ([, path, init]) => path.endsWith("/sessions") && init.method === "POST",
  )!;
  expect(call[0]).toBe("travel-laptop");
  expect(JSON.parse(call[2].body as string).workspace_id).toBe("other");
});

test("sidebar device selection keeps the settings section and permits an offline target", async () => {
  window.history.replaceState({}, "", "/office-mac/settings/clients");
  render(MultiDeviceDashboardHarness);
  await screen.findByText("Device page");
  expect(screen.queryByLabelText("Settings device")).not.toBeInTheDocument();
  fixture.connectionFailures.get("travel-laptop")!(new Error("offline"));
  await chooseSidebarDevice("Travel Laptop");
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/clients"));
  await userEvent.click(await screen.findByRole("button", { name: "Try again" }));
  await screen.findByText("Device page");
  expect(fixture.signals.get("office-mac")).toHaveLength(1);
});

test("root settings opens the active device", async () => {
  window.history.replaceState({}, "", "/travel-laptop/chat/same");
  render(MultiDeviceDashboardHarness);
  await screen.findByText("Device page");
  await userEvent.click(screen.getByRole("button", { name: /New Chat/ }));
  await screen.findByRole("textbox");
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/settings/common"));
});

test("an offline active device on the root can retry", async () => {
  fixture.connect.mockRejectedValue(new Error("offline"));
  render(MultiDeviceDashboardHarness);
  const retry = await screen.findByRole("button", { name: "Try again" });
  fixture.connect.mockImplementation(async (handle) => fixture.target(handle));
  await userEvent.click(retry);
  await screen.findByRole("textbox");
});

test("session shortcuts on the root use the active device", async () => {
  localStorage.setItem("pontia.lastDeviceHandle", "travel-laptop");
  render(MultiDeviceDashboardHarness);
  await screen.findByRole("textbox");
  await sidebar().findByRole("button", { name: "travel-laptop chat" });
  await fireEvent.keyDown(window, { key: "1", code: "Digit1", altKey: true });
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop/chat/same"));
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
  await chooseSidebarDevice("Travel Laptop");
  await waitFor(() => expect(location.pathname).toBe("/travel-laptop"));
  await screen.findByRole("textbox");
  expect(
    screen.queryByRole("button", { name: "Remove attachment pending.txt" }),
  ).not.toBeInTheDocument();
  await chooseSidebarDevice("Office Mac");
  await waitFor(() => expect(location.pathname).toBe("/office-mac"));
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
    '<user_uploads note="Temporary files; copy any you need into the workspace.">\n<user_upload path="/office-mac/pending.txt">pending.txt</user_upload>\n</user_uploads>',
  );
});

test("root login expiry redirects to sign-in and cannot restart device connections", async () => {
  vi.mocked(fixture.remote.listPublicDevices).mockRejectedValueOnce(
    new fixture.remote.CloudRequestError("unauthorized", 401),
  );
  render(MultiDeviceDashboardHarness);
  await waitFor(() => expect(redirectToSignIn).toHaveBeenCalledTimes(1));
  expect(redirectToSignIn.mock.calls[0][0]).toContain("https://pontia.dev/login?return_to=");
  expect(screen.getByRole("link", { name: "Sign in" })).toHaveAttribute(
    "href",
    redirectToSignIn.mock.calls[0][0],
  );
  expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  await fireEvent.focus(window);
  expect(fixture.connect).not.toHaveBeenCalled();
  expect(fixture.remote.listPublicDevices).toHaveBeenCalledTimes(1);
});

test("an account without devices shows setup steps and finds a newly registered device", async () => {
  fixture.listed.splice(0);
  render(MultiDeviceDashboardHarness);
  expect(await screen.findByRole("heading", { name: "Connect your first device" })).toBeVisible();
  expect(screen.getByText(/pontia" init/)).toBeVisible();
  expect(redirectToSignIn).not.toHaveBeenCalled();
  fixture.listed.push({ handle: "office-mac", name: "Office Mac" });
  await fireEvent.click(screen.getByRole("button", { name: "Check for devices" }));
  await waitFor(() =>
    expect(fixture.connect).toHaveBeenCalledWith("office-mac", expect.anything()),
  );
  await waitFor(() =>
    expect(
      screen.queryByRole("heading", { name: "Connect your first device" }),
    ).not.toBeInTheDocument(),
  );
});
