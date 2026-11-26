import "./shallowRouting";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import AgentClientsPage from "../src/pages/AgentClientsPage.svelte";
import MobileClientsSettings from "../src/views/mobile/settings/MobileClientsSettings.svelte";
import MobileSettingsHost from "./components/layout/MobileSettingsHost.svelte";
import type { AgentClientView } from "../src/api/types";

const api = vi.hoisted(() => ({
  listAgentClients: vi.fn(),
  listAgentProfiles: vi.fn(),
  patchAgentClient: vi.fn(),
  resetAgentClient: vi.fn(),
  setupAgentClient: vi.fn(),
  getAgentClientSetup: vi.fn(),
  installAgentClient: vi.fn(),
  getLatestAgentClientOperation: vi.fn(),
  getAgentClientOperation: vi.fn(),
  cancelAgentClientOperation: vi.fn(),
  streamAgentClientOperation: vi.fn(),
  getAgentClientAuth: vi.fn(),
}));
vi.mock("../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = await (() => api)(importOriginal);
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});
const peer = (key: string): AgentClientView => ({
  id: `opaque-${key}`,
  client_type: "acp",
  client_key: key,
  short_id: key,
  name: key,
  enabled: true,
  detection: { status: "unknown", detail: null },
  setup: { status: "not_required" },
  setup_command: null,
  auth_supported: false,
  version: null,
  version_checked_at: null,
  has_record: true,
  created_at: null,
  updated_at: null,
});

const pi = { ...peer("pi"), client_type: "pi", client_key: "default" };
const nav = () => screen.getByRole("navigation", { name: "Agent clients" });

beforeEach(() => {
  vi.resetAllMocks();
  api.getAgentClientSetup.mockResolvedValue(null);
  api.getLatestAgentClientOperation.mockResolvedValue(null);
  api.listAgentProfiles.mockResolvedValue([]);
  api.listAgentClients.mockResolvedValue([peer("grok"), pi]);
});

test("a client shows its settings without a configuration editor or internal record details", async () => {
  api.listAgentClients.mockResolvedValue([
    {
      ...pi,
      detection: { status: "not_detected", detail: null },
      updated_at: "2026-11-24T09:10:27Z",
    },
  ]);
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /pi/ }));
  expect(screen.getByRole("switch", { name: "Enable pi" })).toBeEnabled();
  expect(screen.getByText("Not found on this device. Install it, then refresh.")).toBeVisible();
  expect(screen.queryByText("Version")).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByText("Effective configuration")).toBeNull();
  expect(screen.queryByText(/Database record/)).toBeNull();
  expect(screen.queryByText(/pi \/ pi/)).toBeNull();
  expect(screen.queryByText("opaque-pi")).toBeNull();
});

test("status-only authentication keeps the terminal login command available", async () => {
  api.listAgentClients.mockResolvedValue([
    {
      ...pi,
      auth_supported: true,
      detection: { status: "detected", detail: "/usr/bin/pi" },
      install: {
        supported: true,
        provenance: "vendor_native",
        available_actions: [],
        methods: [],
        auth: {
          status: "not_configured",
          login_command: "pi auth login openai",
          headless_login_command: null,
          env_vars: [],
          docs_url: null,
        },
      },
    },
  ]);
  api.getAgentClientAuth.mockResolvedValue({
    state: "unauthenticated",
    checked_at: "2026-11-26T00:00:00.000Z",
    methods: [],
    supports_logout: false,
  });
  render(AgentClientsPage);
  expect(await screen.findByText("pi auth login openai")).toBeVisible();
  expect(api.getAgentClientAuth).toHaveBeenCalledWith(
    expect.objectContaining({ client_type: "pi", client_key: "default" }),
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
});

test("the first client is selected and the list shows one status per client, detected clients first", async () => {
  api.listAgentClients.mockResolvedValue([
    { ...peer("kiro"), name: "Kiro", detection: { status: "not_detected", detail: null } },
    {
      ...pi,
      name: "Codex",
      client_type: "codex",
      detection: { status: "detected", detail: "/usr/bin/codex" },
      setup: { status: "required", detail: "Missing daemon" },
      setup_command: "pontia setup codex",
    },
    {
      ...peer("grok"),
      name: "Grok",
      detection: { status: "detected", detail: null },
      enabled: false,
    },
    { ...peer("ag"), name: "Antigravity", detection: { status: "detected", detail: null } },
  ]);
  render(AgentClientsPage);
  expect(await screen.findByRole("heading", { name: "Codex" })).toBeVisible();
  const items = within(nav()).getAllByRole("button");
  expect(items.map((item) => item.textContent?.replace(/\s+/g, " ").trim())).toEqual([
    "Codex Needs initialization",
    "Grok Disabled",
    "Antigravity Enabled",
    "Kiro Not detected",
  ]);
  expect(nav()).toHaveTextContent("Detected · 3");
  expect(nav()).toHaveTextContent("Not detected · 1");
  expect(items[0]).toHaveAttribute("aria-current", "true");
  expect(screen.getByText("Needs initialization: Missing daemon")).toBeVisible();
  expect(screen.getByText("pontia setup codex")).toBeVisible();
  expect(screen.getByText("/usr/bin/codex")).toBeVisible();
});

test("a client shows its detected version only when one is recorded", async () => {
  api.listAgentClients.mockResolvedValue([{ ...pi, name: "Pi", version: "0.70.2" }, peer("grok")]);
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Pi/ }));
  expect(screen.getByText("0.70.2")).toBeInTheDocument();
  expect(nav()).not.toHaveTextContent("0.70.2");
  await user.click(screen.getByRole("button", { name: /grok/ }));
  expect(screen.queryByText("0.70.2")).toBeNull();
});

test("default model and parameters link to Agent profiles with the client's profile count", async () => {
  api.listAgentProfiles.mockResolvedValue([
    { profile_id: "a", client_type: "pi", client_key: "default" },
    { profile_id: "b", client_type: "pi", client_key: "default" },
    { profile_id: "c", client_type: "acp", client_key: "grok" },
  ]);
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^pi/ }));
  const link = await screen.findByRole("link", { name: /Default model and parameters/ });
  await waitFor(() => expect(link).toHaveTextContent("2 profiles"));
  expect(link).toHaveAttribute("href", expect.stringMatching(/\/settings\/agent-profiles$/));
});

test("disable sends only the enabled setting and reset removes the entire selected record", async () => {
  const user = userEvent.setup();
  api.patchAgentClient.mockImplementation(async (target, patch) => ({ ...target, ...patch }));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /pi/ }));
  await user.click(screen.getByRole("switch", { name: "Enable pi" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "default" }),
      { enabled: false },
    ),
  );
  expect(screen.getByRole("switch", { name: "Enable pi" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "Restore default" }));
  const dialog = screen.getByRole("alertdialog");
  await user.click(within(dialog).getByRole("button", { name: "Restore default" }));
  await waitFor(() =>
    expect(api.resetAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "default" }),
    ),
  );
});

test("a failed toggle shows the error", async () => {
  const user = userEvent.setup();
  api.patchAgentClient.mockRejectedValue(new Error("Rejected"));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /pi/ }));
  await user.click(screen.getByRole("switch", { name: "Enable pi" }));
  expect(await screen.findByText("Rejected")).toBeInTheDocument();
});

test("a failed load shows the error with a retry", async () => {
  const user = userEvent.setup();
  api.listAgentClients.mockRejectedValueOnce(new Error("Device is offline"));
  render(AgentClientsPage);
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Device is offline");
  await user.click(within(alert).getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("button", { name: /grok/ })).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("Grok can be enabled and reset", async () => {
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /grok/ }));
  expect(screen.queryByRole("button", { name: "Add custom client" })).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByText("Pontia integration")).toBeNull();
  expect(screen.getByRole("switch", { name: "Enable grok" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Restore default" }));
  await user.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Restore default" }),
  );
  await waitFor(() =>
    expect(api.resetAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "grok" }),
    ),
  );
});

test("an available installer is confirmed and starts with automatic setup and enablement", async () => {
  const user = userEvent.setup();
  const method = {
    id: "official_script",
    label: "Official installer",
    recommended: true,
    supported_on_platform: true,
    source_summary: "https://example.test/install.sh",
    target_summary: "~/.local/bin/kiro-cli",
    prerequisites: [
      {
        id: "bash",
        label: "bash",
        required: true,
        satisfied: true,
        detected_detail: "/usr/bin/bash",
        missing_hint: null,
      },
    ],
    ready: true,
  };
  const client: AgentClientView = {
    ...peer("kiro"),
    name: "Kiro",
    enabled: false,
    detection: { status: "not_detected", detail: null },
    install: {
      supported: true,
      provenance: "not_installed",
      available_actions: ["install"],
      methods: [method],
      auth: {
        status: "not_configured",
        login_command: "kiro-cli login",
        headless_login_command: null,
        env_vars: [],
        docs_url: null,
      },
    },
  };
  api.listAgentClients.mockResolvedValue([client]);
  api.installAgentClient.mockResolvedValue({
    id: "install-1",
    client_type: "acp",
    client_key: "kiro",
    method_id: method.id,
    phase: "done",
    status: "succeeded",
    cancel_requested: false,
    error: null,
    log_size: 8,
    output_offset: 0,
    next_offset: 8,
    output: "verified",
  });
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Kiro/ }));
  await user.click(screen.getByRole("button", { name: "Install", exact: true }));
  const dialog = screen.getByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "Install", exact: true }));
  await waitFor(() => expect(api.installAgentClient).toHaveBeenCalledWith(client, method.id));
  expect(screen.getByRole("button", { name: "View output" })).toBeEnabled();
});

const installOperation = (
  status: "running" | "succeeded" | "failed" | "cancelled" = "running",
) => ({
  id: "install-1",
  client_type: "acp",
  client_key: "kiro",
  method_id: "official_script",
  phase: status === "succeeded" ? ("done" as const) : ("installing" as const),
  status,
  cancel_requested: false,
  error: status === "failed" ? "Process exited with 1" : null,
  log_size: 6,
  output_offset: 0,
  next_offset: 6,
  output: "first\n",
});

test("cancelling an installation requires confirmation and keeps the dialog open while cancellation is pending", async () => {
  const user = userEvent.setup();
  const client = { ...peer("kiro"), name: "Kiro" };
  const running = installOperation();
  api.listAgentClients.mockResolvedValue([client]);
  api.getLatestAgentClientOperation.mockResolvedValue(running);
  api.streamAgentClientOperation.mockImplementation(() => new Promise(() => {}));
  api.cancelAgentClientOperation.mockResolvedValue({ ...running, cancel_requested: true });
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: "Cancel" }));
  const dialog = screen.getByRole("alertdialog");
  await user.click(within(dialog).getByRole("button", { name: "Cancel installation" }));
  await waitFor(() => expect(api.cancelAgentClientOperation).toHaveBeenCalledWith(running.id));
  expect(screen.getByRole("alertdialog")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancelling…" })).toBeDisabled();
});

test("another running operation disables installation retry", async () => {
  const client = { ...peer("kiro"), name: "Kiro" };
  api.listAgentClients.mockResolvedValue([client, setupClient()]);
  api.getLatestAgentClientOperation.mockResolvedValue(installOperation("failed"));
  api.getAgentClientSetup.mockResolvedValue(operation("running"));
  render(AgentClientsPage);
  const retry = await screen.findByRole("button", { name: "Retry" });
  expect(retry).toBeDisabled();
});

test("a disconnected installation stream resumes from the next byte offset", async () => {
  const client = { ...peer("kiro"), name: "Kiro" };
  const running = installOperation();
  api.listAgentClients.mockResolvedValue([client]);
  api.getLatestAgentClientOperation.mockResolvedValue(running);
  api.streamAgentClientOperation
    .mockRejectedValueOnce(new TypeError("Failed to fetch"))
    .mockImplementation(() => new Promise(() => {}));
  api.getAgentClientOperation.mockResolvedValue({
    ...running,
    log_size: 13,
    output_offset: 6,
    next_offset: 13,
    output: "second\n",
  });
  render(AgentClientsPage);
  await waitFor(
    () =>
      expect(api.streamAgentClientOperation).toHaveBeenLastCalledWith(
        running.id,
        13,
        expect.any(AbortSignal),
      ),
    { timeout: 4000 },
  );
  expect(api.getAgentClientOperation).toHaveBeenCalledWith(running.id, 6, {
    signal: expect.any(AbortSignal),
  });
});

test("a reopened completed installation loads output omitted from the latest tail snapshot", async () => {
  const user = userEvent.setup();
  const client = { ...peer("kiro"), name: "Kiro" };
  const completed = {
    ...installOperation("succeeded"),
    log_size: 70_000,
    output_offset: 65_536,
    next_offset: 70_000,
    output: "tail",
  };
  api.listAgentClients.mockResolvedValue([client]);
  api.getLatestAgentClientOperation.mockResolvedValue(completed);
  api.getAgentClientOperation.mockImplementation(async (_id, offset) =>
    offset === 0
      ? { ...completed, output_offset: 0, next_offset: 65_536, output: "head" }
      : completed,
  );
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: "View output" }));
  expect(within(screen.getByRole("dialog")).getByText("headtail")).toBeVisible();
  expect(api.getAgentClientOperation).toHaveBeenNthCalledWith(1, completed.id, 0);
  expect(api.getAgentClientOperation).toHaveBeenNthCalledWith(2, completed.id, 65_536);
});

const setupClient = (): AgentClientView => ({
  ...peer("default"),
  client_type: "pi",
  name: "Pi",
  enabled: false,
  detection: { status: "unknown", detail: null },
  setup: { status: "required", detail: "Missing extension" },
  setup_command: "pontia setup pi",
});
const operation = (status: "running" | "succeeded" | "failed") => ({
  id: "setup-1",
  client_type: "pi",
  client_key: "default",
  status,
  error: status === "failed" ? "Process exited with 1" : null,
  output: "Setup output",
});

test("setup enables the selected client", async () => {
  const user = userEvent.setup();
  api.listAgentClients.mockResolvedValue([setupClient()]);
  api.setupAgentClient.mockResolvedValue(operation("running"));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Pi/ }));
  await user.click(screen.getByRole("button", { name: "Initialize" }));
  expect(api.setupAgentClient).toHaveBeenCalledWith(setupClient());
  expect(screen.getByRole("button", { name: "Initializing…" })).toBeDisabled();
  expect(within(nav()).getByRole("button", { name: /^Pi/ })).toHaveTextContent("Initializing…");
  api.listAgentClients.mockResolvedValue([
    { ...setupClient(), enabled: true, setup: { status: "initialized" } },
  ]);
  api.getAgentClientSetup.mockResolvedValue(operation("succeeded"));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Enable Pi" })).toBeChecked(), {
    timeout: 4000,
  });
  expect(screen.getByText("Initialization completed. The client is enabled.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Reinitialize" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "View output" }));
  expect(within(screen.getByRole("dialog")).getByText("Setup output")).toBeVisible();
});

test("other clients stay selectable while setup runs, but cannot start another setup", async () => {
  const user = userEvent.setup();
  const other = { ...setupClient(), id: "other", client_type: "codex", name: "Codex" };
  api.listAgentClients.mockResolvedValue([setupClient(), other]);
  api.getAgentClientSetup.mockResolvedValue(operation("running"));
  render(AgentClientsPage);
  await waitFor(() => expect(screen.getByRole("button", { name: "Initializing…" })).toBeDisabled());
  await user.click(within(nav()).getByRole("button", { name: /^Codex/ }));
  expect(screen.getByRole("heading", { name: "Codex" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Initialize" })).toBeDisabled();
  expect(within(nav()).getByRole("button", { name: /^Pi/ })).toHaveTextContent("Initializing…");
});

test("a reopened page recovers a running setup across a temporary disconnect and permits retry after failure", async () => {
  const user = userEvent.setup();
  api.listAgentClients.mockResolvedValue([setupClient()]);
  api.getAgentClientSetup
    .mockResolvedValueOnce(operation("running"))
    .mockRejectedValueOnce(new TypeError("Failed to fetch"))
    .mockResolvedValue(operation("failed"));
  api.setupAgentClient.mockResolvedValue({ ...operation("running"), id: "retry" });
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Pi/ }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Initializing…" })).toBeDisabled());
  await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled(), {
    timeout: 6000,
  });
  expect(screen.getByText("Initialization failed: Process exited with 1")).toBeVisible();
  expect(within(nav()).getByRole("button", { name: /^Pi/ })).toHaveTextContent(
    "Initialization failed",
  );
  expect(screen.getByRole("switch", { name: "Enable Pi" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(api.setupAgentClient).toHaveBeenCalledTimes(1);
});

test("phones refresh from the top bar and return from a client with its back button", async () => {
  window.history.pushState({}, "", "/dashboard/settings/clients");
  const user = userEvent.setup();
  render(MobileSettingsHost, { props: { page: MobileClientsSettings } });
  await user.click(await screen.findByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(api.listAgentClients).toHaveBeenCalledTimes(2));
  await user.click(await screen.findByRole("button", { name: /^grok/ }));
  expect(screen.getByRole("switch", { name: "Enable grok" })).toBeChecked();
  await user.click(
    within(screen.getAllByRole("banner").at(-1)!).getByRole("button", { name: "Back" }),
  );
  await waitFor(() => expect(screen.queryByRole("switch")).toBeNull());
  expect(screen.getByRole("button", { name: /^grok/ })).toBeEnabled();
});
