import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import AgentClientsPage from "../src/pages/AgentClientsPage.svelte";
import type { AgentClientView } from "../src/api/types";

const api = vi.hoisted(() => ({
  listAgentClients: vi.fn(),
  patchAgentClient: vi.fn(),
  resetAgentClient: vi.fn(),
  setupAgentClient: vi.fn(),
  getAgentClientSetup: vi.fn(),
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
  setup_command: null,
  has_record: true,
  config_overrides: { name: key },
  effective_config: { name: key, command: "inherited-command" },
  created_at: null,
  updated_at: null,
});

beforeEach(() => {
  vi.resetAllMocks();
  api.getAgentClientSetup.mockResolvedValue(null);
  api.listAgentClients.mockResolvedValue([peer("grok"), peer("opencode")]);
});

test("a client that is not detected remains configurable and saves only its database overrides", async () => {
  api.listAgentClients.mockResolvedValue([
    { ...peer("opencode"), detection: { status: "not_detected", detail: null } },
  ]);
  const user = userEvent.setup();
  api.patchAgentClient.mockImplementation(async (target, patch) => ({
    ...target,
    config_overrides: patch.config_overrides,
  }));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /opencode/ }));
  const editor = screen.getByLabelText("Database overrides (JSON)");
  expect(editor).toHaveValue(JSON.stringify({ name: "opencode" }, null, 2));
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"name":"Renamed"}');
  await user.click(screen.getByRole("button", { name: "Save overrides" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_type: "acp", client_key: "opencode" }),
      { config_overrides: { name: "Renamed" } },
    ),
  );
});

test("disable preserves the edited override draft and reset removes the entire selected record", async () => {
  const user = userEvent.setup();
  api.patchAgentClient.mockImplementation(async (target, patch) => ({ ...target, ...patch }));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /grok/ }));
  const editor = screen.getByLabelText("Database overrides (JSON)");
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"name":"Unsaved"}');
  await user.click(screen.getByRole("switch", { name: "Enabled" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "grok" }),
      { enabled: false },
    ),
  );
  expect(editor).toHaveValue('{"name":"Unsaved"}');
  await user.click(screen.getByRole("button", { name: "Restore inheritance" }));
  const dialog = screen.getByRole("alertdialog");
  await user.click(within(dialog).getByRole("button", { name: "Restore inheritance" }));
  await waitFor(() =>
    expect(api.resetAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "grok" }),
    ),
  );
});

test("invalid JSON and failed saves keep the user draft without modifying another peer", async () => {
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /grok/ }));
  const editor = screen.getByLabelText("Database overrides (JSON)");
  await user.clear(editor);
  await user.click(editor);
  await user.paste("[]");
  await user.click(screen.getByRole("button", { name: "Save overrides" }));
  await screen.findByRole("alert");
  expect(api.patchAgentClient).not.toHaveBeenCalled();
  api.patchAgentClient.mockRejectedValue(new Error("Rejected"));
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"command":null}');
  await user.click(screen.getByRole("button", { name: "Save overrides" }));
  await waitFor(() => expect(api.patchAgentClient).toHaveBeenCalledTimes(1));
  expect(editor).toHaveValue('{"command":null}');
});

test("reserved dot IDs are rejected before a custom client request is sent", async () => {
  const user = userEvent.setup();
  render(AgentClientsPage);
  await screen.findByRole("button", { name: /grok/ });
  await user.click(screen.getByRole("button", { name: "Add custom client" }));
  const key = screen.getByLabelText("ACP Agent ID");
  for (const id of [".", ".."]) {
    await user.clear(key);
    await user.type(key, id);
    await user.click(screen.getByRole("button", { name: "Save overrides" }));
    await screen.findByRole("alert");
    expect(api.patchAgentClient).not.toHaveBeenCalled();
    expect(key).toHaveValue(id);
  }
});

test("custom clients use their stable key rather than their display name", async () => {
  const user = userEvent.setup();
  api.patchAgentClient.mockResolvedValue(peer("company-agent"));
  render(AgentClientsPage);
  await screen.findByRole("button", { name: /grok/ });
  await user.click(screen.getByRole("button", { name: "Add custom client" }));
  await user.type(screen.getByLabelText("ACP Agent ID"), "company-agent");
  const editor = screen.getByLabelText("Database overrides (JSON)");
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"name":"Company Agent","command":"agent"}');
  await user.click(screen.getByRole("button", { name: "Save overrides" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      { client_type: "acp", client_key: "company-agent" },
      { config_overrides: { name: "Company Agent", command: "agent" } },
    ),
  );
});

const setupClient = (): AgentClientView => ({
  ...peer("default"),
  client_type: "pi",
  name: "Pi",
  enabled: false,
  detection: { status: "unknown", detail: null },
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

test("setup enables the selected client and preserves an unsaved override draft", async () => {
  const user = userEvent.setup();
  api.listAgentClients.mockResolvedValue([setupClient()]);
  api.setupAgentClient.mockResolvedValue(operation("running"));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Pi/ }));
  const editor = screen.getByLabelText("Database overrides (JSON)");
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"name":"Unsaved"}');
  await user.click(screen.getByRole("button", { name: "Initialize" }));
  expect(api.setupAgentClient).toHaveBeenCalledWith(setupClient());
  expect(screen.getByRole("button", { name: "Initializing…" })).toBeDisabled();
  api.listAgentClients.mockResolvedValue([{ ...setupClient(), enabled: true }]);
  api.getAgentClientSetup.mockResolvedValue(operation("succeeded"));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Enabled" })).toBeChecked(), {
    timeout: 4000,
  });
  expect(editor).toHaveValue('{"name":"Unsaved"}');
  expect(screen.getByRole("button", { name: "Initialize" })).toBeEnabled();
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
  await waitFor(() => expect(screen.getByRole("button", { name: "Initialize" })).toBeEnabled(), {
    timeout: 6000,
  });
  expect(screen.getByRole("switch", { name: "Enabled" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "Initialize" }));
  expect(api.setupAgentClient).toHaveBeenCalledTimes(1);
});
