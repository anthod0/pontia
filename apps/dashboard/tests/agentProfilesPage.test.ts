import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import AgentProfilesPage from "../src/pages/AgentProfilesPage.svelte";
import { token } from "../src/stores/auth";
import { createSampleData } from "../mock/data";
import { createMockDevice, handleDeviceRequest, type MockDevice } from "../mock/server";
import { testDevice } from "./deviceFixture";

let device: MockDevice;
beforeEach(() => {
  device = createMockDevice(createSampleData());
  token.set("test-token");
  testDevice.queryClient.setDefaultOptions({ queries: { retry: false } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(new URL(String(input), "http://localhost"), init);
      return handleDeviceRequest(
        device,
        request,
        new URL(request.url).pathname.replace("/api/v1", ""),
      );
    }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.style.pointerEvents = "";
});

const list = () => screen.getByRole("navigation", { name: "Agent Profiles" });
const open = async (name: string) =>
  fireEvent.click(
    await within(await screen.findByRole("navigation", {}, { timeout: 3000 })).findByRole(
      "button",
      {
        name: new RegExp(`^${name}`),
      },
    ),
  );

/** Chooses a client by keyboard and returns the names of the clients offered. */
async function chooseClient(name: string) {
  const trigger = screen.getByRole("combobox", { name: "Agent client" });
  await fireEvent.keyDown(trigger, { key: "ArrowDown" });
  await screen.findByRole("option", { name, exact: true });
  const names = screen.getAllByRole("option").map((option) => option.textContent?.trim());
  await fireEvent.keyDown(trigger, { key: "Home" });
  for (let i = 0; i < names.indexOf(name); i++)
    await fireEvent.keyDown(trigger, { key: "ArrowDown" });
  await fireEvent.keyDown(trigger, { key: "Enter" });
  return names;
}

async function chooseModel(index: number) {
  await fireEvent.click(screen.getByRole("button", { name: "Model" }));
  const models = within(await screen.findByRole("list", { name: "Available models" }));
  await fireEvent.click(models.getAllByRole("button")[index]);
}

test("creates the first profile with an ID suggested from its name", async () => {
  const user = userEvent.setup();
  device.data.agentProfiles = [];
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "Release Writer");
  expect(screen.getByLabelText("Profile ID")).toHaveValue("release-writer");
  await user.type(screen.getByLabelText("System prompt"), "Write clearly.");
  await screen.findByRole("button", { name: "Model" });
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await within(list()).findByRole("button", { name: /^Release Writer/ });
  expect(device.data.agentProfiles).toEqual([
    expect.objectContaining({
      profile_id: "release-writer",
      name: "Release Writer",
      client_type: "pi",
      client_key: "pi",
      model: "anthropic/claude-sonnet-4-5",
      system_prompt: "Write clearly.",
    }),
  ]);
});

test("does not create a profile whose ID pontiad would reject", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "评审");
  expect(screen.getByLabelText("Profile ID")).toHaveValue("");
  await user.type(screen.getByLabelText("Profile ID"), "Code Review");
  await screen.findByRole("button", { name: "Model" });
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  expect(screen.getByLabelText("Profile ID")).toHaveAttribute("aria-invalid", "true");
  expect(device.data.agentProfiles).toHaveLength(2);
  await user.clear(screen.getByLabelText("Profile ID"));
  await user.type(screen.getByLabelText("Profile ID"), "code-review");
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await waitFor(() =>
    expect(device.data.agentProfiles.map((p) => p.profile_id)).toContain("code-review"),
  );
});

test("switches a profile to another enabled client and model", async () => {
  device.data.agentClients.find((c) => c.client_type === "claude-agent")!.enabled = false;
  render(AgentProfilesPage);
  await open("Executor");
  expect(await chooseClient("Codex")).not.toContain("Claude Code");
  await chooseModel(2);
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      profile_id: "executor",
      client_type: "codex",
      client_key: "codex",
      model: "openai/gpt-5-codex",
    }),
  );
});

test("keeps entered values after a failed save so the user can correct and retry", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "Executor");
  await screen.findByRole("button", { name: "Model" });
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await screen.findByRole("alert");
  expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Executor");
  await waitFor(() => expect(screen.getByLabelText("Profile ID")).toBeEnabled());
  await user.clear(screen.getByLabelText("Profile ID"));
  await user.type(screen.getByLabelText("Profile ID"), "executor-2");
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await waitFor(() =>
    expect(device.data.agentProfiles.find((p) => p.profile_id === "executor-2")?.name).toBe(
      "Executor",
    ),
  );
});

test("edits and deletes an existing profile whose ID predates the ID format", async () => {
  const user = userEvent.setup();
  device.data.agentProfiles[0].profile_id = "Old Executor";
  render(AgentProfilesPage);
  await open("Executor");
  expect(screen.getByLabelText("Profile ID")).toHaveAttribute("readonly");
  await user.clear(screen.getByLabelText("Name", { exact: true }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "Builder");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      profile_id: "Old Executor",
      name: "Builder",
    }),
  );
  // Dialog steps use fireEvent: the closing dialog's body scroll lock outlasts user-event's checks.
  await fireEvent.click(screen.getByRole("button", { name: "Delete Builder" }));
  await fireEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancel" }),
  );
  expect(device.data.agentProfiles.some((p) => p.profile_id === "Old Executor")).toBe(true);
  await fireEvent.click(screen.getByRole("button", { name: "Delete Builder" }));
  await fireEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete", exact: true }),
  );
  await waitFor(() =>
    expect(device.data.agentProfiles.some((p) => p.profile_id === "Old Executor")).toBe(false),
  );
  await waitFor(() =>
    expect(within(list()).queryByRole("button", { name: /^Builder/ })).not.toBeInTheDocument(),
  );
});

test("does not save a profile whose saved client is no longer available", async () => {
  const user = userEvent.setup();
  Object.assign(device.data.agentProfiles[0], {
    client_type: "removed",
    client_key: "old",
    model: "old-model",
  });
  render(AgentProfilesPage);
  await open("Executor");
  const updates = vi.mocked(fetch).mock.calls.length;
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(
    vi
      .mocked(fetch)
      .mock.calls.slice(updates)
      .map(([, init]) => init?.method),
  ).not.toContain("PUT");
  expect(device.data.agentProfiles[0]).toMatchObject({
    client_type: "removed",
    model: "old-model",
  });
  await chooseClient("Pi");
  await chooseModel(0);
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      client_type: "pi",
      client_key: "pi",
      model: "anthropic/claude-sonnet-4-5",
    }),
  );
});

test("cannot create a profile while no client is enabled", async () => {
  for (const client of device.data.agentClients) client.enabled = false;
  render(AgentProfilesPage);
  const newProfile = await screen.findByRole("button", { name: "New profile" });
  await waitFor(() => expect(newProfile).toBeEnabled());
  await fireEvent.click(newProfile);
  expect(screen.getByRole("button", { name: "Create profile" })).toBeDisabled();
});

test("reads an individual profile and cannot read it after deletion", async () => {
  const profile = await testDevice.api.getAgentProfile("executor");
  expect(profile).toMatchObject({ profile_id: "executor", name: "Executor" });
  await testDevice.api.deleteAgentProfile("executor");
  await expect(testDevice.api.getAgentProfile("executor")).rejects.toMatchObject({ status: 404 });
});
