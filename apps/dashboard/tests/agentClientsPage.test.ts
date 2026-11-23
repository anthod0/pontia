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
  created_at: null,
  updated_at: null,
});

const pi = { ...peer("pi"), client_type: "pi", client_key: "default" };

beforeEach(() => {
  vi.resetAllMocks();
  api.getAgentClientSetup.mockResolvedValue(null);
  api.listAgentClients.mockResolvedValue([peer("grok"), pi]);
});

test("a client shows its settings without a configuration editor or effective configuration", async () => {
  api.listAgentClients.mockResolvedValue([
    { ...pi, detection: { status: "not_detected", detail: null } },
  ]);
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /pi/ }));
  expect(screen.getByRole("switch", { name: "Enabled" })).toBeEnabled();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByText("Effective configuration")).toBeNull();
});

test("disable sends only the enabled setting and reset removes the entire selected record", async () => {
  const user = userEvent.setup();
  api.patchAgentClient.mockImplementation(async (target, patch) => ({ ...target, ...patch }));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /pi/ }));
  await user.click(screen.getByRole("switch", { name: "Enabled" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "default" }),
      { enabled: false },
    ),
  );
  expect(screen.getByRole("switch", { name: "Enabled" })).not.toBeChecked();
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
  await user.click(screen.getByRole("switch", { name: "Enabled" }));
  expect(await screen.findByText("Rejected")).toBeInTheDocument();
});

test("Grok can be enabled and reset", async () => {
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /grok/ }));
  expect(screen.queryByRole("button", { name: "Add custom client" })).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.getByRole("switch", { name: "Enabled" })).toBeEnabled();
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

test("setup enables the selected client", async () => {
  const user = userEvent.setup();
  api.listAgentClients.mockResolvedValue([setupClient()]);
  api.setupAgentClient.mockResolvedValue(operation("running"));
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /^Pi/ }));
  await user.click(screen.getByRole("button", { name: "Initialize" }));
  expect(api.setupAgentClient).toHaveBeenCalledWith(setupClient());
  expect(screen.getByRole("button", { name: "Initializing…" })).toBeDisabled();
  api.listAgentClients.mockResolvedValue([{ ...setupClient(), enabled: true }]);
  api.getAgentClientSetup.mockResolvedValue(operation("succeeded"));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Enabled" })).toBeChecked(), {
    timeout: 4000,
  });
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
