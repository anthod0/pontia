import "./shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { publishedTemplatesUrl } from "../src/pages/agentProfiles/publishedTemplates";
import AgentProfilesPage from "../src/pages/AgentProfilesPage.svelte";
import { token } from "../src/stores/auth";
import { createSampleData } from "../mock/data";
import { createMockDevice, handleDeviceRequest, type MockDevice } from "../mock/server";
import { testDevice } from "./deviceFixture";
import MobileProfilesSettings from "../src/views/mobile/settings/MobileProfilesSettings.svelte";
import MobileSettingsHost from "./components/layout/MobileSettingsHost.svelte";

const sampleTemplate = (id: string, name: string, client_type: string, model: string) => ({
  id,
  icon: "code",
  client_type,
  model,
  reasoning_effort: "medium",
  use_yolo: true,
  text: { en: { name, description: "Test description", system_prompt: "Test prompt." } },
});

const templateDocument = {
  schema_version: 1,
  templates: [
    sampleTemplate("sample", "Sample", "pi", "anthropic/claude-sonnet-5-5"),
    sampleTemplate("review", "Review sample", "codex", "unlisted"),
    sampleTemplate("notes", "Notes sample", "claude-agent", "sonnet"),
  ],
};

let device: MockDevice;
beforeEach(() => {
  device = createMockDevice(createSampleData());
  token.set("test-token");
  testDevice.queryClient.setDefaultOptions({ queries: { retry: false } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(new URL(String(input), "http://localhost"), init);
      if (request.url === publishedTemplatesUrl) return Response.json(templateDocument);
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

/** Picks a model; models that offer efforts keep the picker open until an effort is chosen. */
async function chooseModel(index: number) {
  await fireEvent.click(screen.getByRole("button", { name: "Model" }));
  const models = within(await screen.findByLabelText("Available models"));
  await fireEvent.click((await models.findAllByRole("option", { hidden: true }))[index]);
  const efforts = screen.queryByLabelText("Reasoning effort");
  if (efforts) await fireEvent.click(within(efforts).getByLabelText("Default"));
}

test("returns to the list after creating the first profile with a suggested ID", async () => {
  const user = userEvent.setup();
  device.data.agentProfiles = [];
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "Release Writer");
  expect(screen.getByLabelText("Profile ID")).toHaveValue("release-writer");
  await user.click(screen.getByRole("switch", { name: "Use YOLO mode" }));
  await user.type(screen.getByLabelText("System prompt"), "Write clearly.");
  await screen.findByRole("button", { name: "Model" });
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await within(list()).findByRole("button", { name: /^Release Writer/ });
  await waitFor(() => expect(screen.queryByLabelText("Profile ID")).toBeNull());
  expect(device.data.agentProfiles).toEqual([
    expect.objectContaining({
      profile_id: "release-writer",
      name: "Release Writer",
      client_type: "pi",
      client_key: "default",
      model: "anthropic/claude-sonnet-4-5",
      use_yolo: true,
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

test("returns to the list after switching a profile to another enabled client and model", async () => {
  device.data.agentClients.find((c) => c.client_type === "claude-agent")!.enabled = false;
  render(AgentProfilesPage);
  await open("Executor");
  const names = await chooseClient("Codex");
  expect(names).not.toContain("Claude Code");
  expect(names).toEqual(["Pi", "Codex", "Google Antigravity", "Grok Build"]);
  await chooseModel(2);
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByLabelText("Profile ID")).toBeNull());
  expect(within(list()).getByRole("button", { name: /^Executor/ })).toBeEnabled();
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      profile_id: "executor",
      client_type: "codex",
      client_key: "default",
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

test("edits and deletes an existing profile whose ID predates the ID format on desktop", async () => {
  const matchMedia = window.matchMedia;
  vi.stubGlobal("matchMedia", (query: string) => ({
    ...matchMedia(query),
    matches: query === "(min-width: 64rem)",
  }));
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
      client_key: "default",
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

test("phones create profiles from the top bar and return from a profile with its back button", async () => {
  window.history.pushState({}, "", "/dashboard/settings/agent-profiles");
  const user = userEvent.setup();
  render(MobileSettingsHost, { props: { page: MobileProfilesSettings } });
  await open("Executor");
  expect(await screen.findByLabelText("Name")).toHaveValue("Executor");
  await user.click(
    within(screen.getAllByRole("banner").at(-1)!).getByRole("button", { name: "Back" }),
  );
  await waitFor(() => expect(screen.queryByLabelText("Name")).toBeNull());
  await user.click(screen.getByRole("button", { name: "New" }));
  expect(await screen.findByLabelText("Name")).toHaveValue("");
});

const card = (name: string) => screen.findByRole("button", { name: new RegExp(`^${name}`) });

test("creates a profile from a template whose client and model are available", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.click(await card("Sample"));
  await screen.findByText("Applied template");
  expect(screen.queryByRole("button", { name: /^Review sample/ })).not.toBeInTheDocument();
  expect(screen.getByLabelText("Profile ID")).toHaveValue("sample");
  expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Sample");
  expect(screen.getByRole("switch", { name: "Use YOLO mode" })).toBeChecked();
  expect((screen.getByLabelText("System prompt") as HTMLTextAreaElement).value).toContain(
    "Test prompt.",
  );
  await screen.findByRole("button", { name: "Model" });
  expect(screen.getByRole("button", { name: "Model" })).toHaveTextContent("Claude Sonnet 5.5");
  expect(screen.queryByText(/template recommends/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Create profile" }));
  await waitFor(() =>
    expect(device.data.agentProfiles.find((p) => p.profile_id === "sample")).toMatchObject({
      name: "Sample",
      client_type: "pi",
      model: "anthropic/claude-sonnet-5-5",
      reasoning_effort: "medium",
      use_yolo: true,
    }),
  );
});

test("leaves the client and model alone when the template's client is not installed", async () => {
  const user = userEvent.setup();
  device.data.agentClients = device.data.agentClients.filter(
    (c) => c.client_type !== "claude-agent",
  );
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  const notesCard = await card("Notes sample");
  expect(notesCard).toHaveTextContent("Not installed");
  await screen.findByRole("button", { name: "Model" });
  await user.click(notesCard);
  const note = await screen.findByText(/which is not installed on this device/);
  expect(note.closest("[role=status]")).toHaveTextContent("Claude Code (sonnet)");
  expect(within(note.closest("[role=status]") as HTMLElement).getByRole("link")).toHaveTextContent(
    "Agent Clients",
  );
  expect(screen.getByRole("combobox", { name: "Agent client" })).toHaveTextContent("Pi");
  expect(screen.getByLabelText("Profile ID")).toHaveValue("notes");
  await chooseClient("Codex");
  await waitFor(() => expect(screen.queryByText(/not installed on this device/)).toBeNull());
});

test("falls back to the client's default model when the template's model is not offered", async () => {
  const user = userEvent.setup();
  device.data.clientModels.codex = [];
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await user.click(await card("Review sample"));
  await waitFor(() =>
    expect(screen.getByRole("status")).toHaveTextContent(
      "Codex does not offer the template's model unlisted",
    ),
  );
  expect(screen.getByRole("combobox", { name: "Agent client" })).toHaveTextContent("Codex");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Model" })).toHaveTextContent("Claude Sonnet 4.5"),
  );
});

test("asks before a template replaces a changed draft", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "New profile" }));
  await screen.findByRole("button", { name: "Model" });
  await user.type(screen.getByLabelText("Name", { exact: true }), "Mine");
  await fireEvent.click(await card("Review sample"));
  const dialog = await screen.findByRole("alertdialog");
  expect(dialog).toHaveTextContent("Applying “Review sample” replaces");
  await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Mine");
  await fireEvent.click(await card("Review sample"));
  await fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Replace" }),
  );
  await waitFor(() =>
    expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Review sample"),
  );

  // An unchanged draft switches templates directly, and the applied template only closes the picker.
  await fireEvent.click(screen.getByRole("button", { name: "Change template" }));
  expect(await card("Review sample")).toHaveAttribute("aria-pressed", "true");
  await fireEvent.click(await card("Review sample"));
  await screen.findByRole("button", { name: "Change template" });
  await fireEvent.click(screen.getByRole("button", { name: "Change template" }));
  await fireEvent.click(await card("Sample"));
  await waitFor(() => expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Sample"));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});

test("starts a profile from a template on the empty state and hides templates while editing", async () => {
  device.data.agentProfiles = [];
  render(AgentProfilesPage);
  await fireEvent.click(await card("Notes sample"));
  await waitFor(() => expect(screen.getByLabelText("Profile ID")).toHaveValue("notes"));
  expect(screen.getByRole("combobox", { name: "Agent client" })).toHaveTextContent("Claude Code");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Model" })).toHaveTextContent("Sonnet"),
  );
});

test("does not offer templates while editing an existing profile", async () => {
  render(AgentProfilesPage);
  await open("Executor");
  await screen.findByRole("button", { name: "Save" });
  expect(screen.queryByText("Start from a template")).not.toBeInTheDocument();
});
