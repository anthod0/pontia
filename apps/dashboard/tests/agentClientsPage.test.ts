import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import AgentClientsPage from "../src/pages/AgentClientsPage.svelte";
import type { AgentClientView } from "../src/api/types";

const api = vi.hoisted(() => ({
  listAgentClients: vi.fn(),
  patchAgentClient: vi.fn(),
  resetAgentClient: vi.fn(),
}));
vi.mock("../src/api/client", () => api);
const peer = (key: string): AgentClientView => ({
  id: `opaque-${key}`,
  client_type: "acp",
  client_key: key,
  short_id: key,
  name: key,
  enabled: true,
  has_record: true,
  config_overrides: { name: key },
  effective_config: { name: key, command: "inherited-command" },
  created_at: null,
  updated_at: null,
});

beforeEach(() => {
  vi.resetAllMocks();
  api.listAgentClients.mockResolvedValue([peer("kimi"), peer("opencode")]);
});

test("editing a flat peer saves only its database overrides, never the effective configuration", async () => {
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
  await user.click(await screen.findByRole("button", { name: /kimi/ }));
  const editor = screen.getByLabelText("Database overrides (JSON)");
  await user.clear(editor);
  await user.click(editor);
  await user.paste('{"name":"Unsaved"}');
  await user.click(screen.getByRole("button", { name: "Disable" }));
  await waitFor(() =>
    expect(api.patchAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "kimi" }),
      { enabled: false },
    ),
  );
  expect(editor).toHaveValue('{"name":"Unsaved"}');
  await user.click(screen.getByRole("button", { name: "Restore inheritance" }));
  const dialog = screen.getByRole("alertdialog");
  await user.click(within(dialog).getByRole("button", { name: "Restore inheritance" }));
  await waitFor(() =>
    expect(api.resetAgentClient).toHaveBeenCalledWith(
      expect.objectContaining({ client_key: "kimi" }),
    ),
  );
});

test("invalid JSON and failed saves keep the user draft without modifying another peer", async () => {
  const user = userEvent.setup();
  render(AgentClientsPage);
  await user.click(await screen.findByRole("button", { name: /kimi/ }));
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
  await screen.findByRole("button", { name: /kimi/ });
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
  await screen.findByRole("button", { name: /kimi/ });
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
