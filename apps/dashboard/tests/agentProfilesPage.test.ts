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

test("creates a profile without launch defaults, reloads it and edits the same resource", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await screen.findByRole("button", { name: "Edit Executor" });
  await user.click(screen.getByRole("button", { name: "New profile" }));
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByLabelText("Profile ID")).toHaveAttribute("aria-invalid", "true");
  expect(device.data.agentProfiles).toHaveLength(2);
  await user.type(screen.getByLabelText("Profile ID"), "writer");
  await user.type(screen.getByLabelText("Name", { exact: true }), "Writer");
  await user.type(screen.getByLabelText("System prompt"), "Write clearly.");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await user.click(await screen.findByRole("button", { name: "Edit Writer" }));
  expect(screen.getByLabelText("Profile ID")).toHaveAttribute("readonly");
  expect(screen.getByLabelText("System prompt")).toHaveValue("Write clearly.");
  await user.clear(screen.getByLabelText("Name", { exact: true }));
  await user.type(screen.getByLabelText("Name", { exact: true }), "Editor");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("button", { name: "Edit Editor" });
  expect(device.data.agentProfiles.filter((p) => p.profile_id === "writer")).toEqual([
    expect.objectContaining({
      name: "Editor",
      client_type: null,
      client_key: null,
      model: null,
      system_prompt: "Write clearly.",
    }),
  ]);
});

test("switches enabled clients and models and clears the complete launch selection", async () => {
  device.data.agentClients.find((c) => c.client_type === "pi")!.enabled = false;
  render(AgentProfilesPage);
  await fireEvent.click(await screen.findByRole("button", { name: "Edit Executor" }));
  await fireEvent.keyDown(screen.getByRole("combobox", { name: "Agent client" }), {
    key: "ArrowDown",
  });
  expect(screen.queryByRole("option", { name: "Pi", exact: true })).not.toBeInTheDocument();
  await screen.findByRole("option", { name: "Codex", exact: true });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "Home" });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "ArrowDown" });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "Enter" });
  await fireEvent.click(screen.getByRole("button", { name: "Model" }));
  const modelButtons = within(
    await screen.findByRole("list", { name: "Available models" }),
  ).getAllByRole("button");
  await fireEvent.click(modelButtons[2]);
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByLabelText("Profile ID")).not.toBeInTheDocument());
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      client_type: "codex",
      client_key: "codex",
      model: "openai/gpt-5-codex",
    }),
  );
  await fireEvent.click(await screen.findByRole("button", { name: "Edit Executor" }));
  await fireEvent.keyDown(screen.getByRole("combobox", { name: "Agent client" }), {
    key: "ArrowDown",
  });
  await screen.findByRole("option", { name: "Claude Code", exact: true });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "End" });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "ArrowUp" });
  await fireEvent.keyDown(screen.getByLabelText("Agent client"), { key: "Enter" });
  await fireEvent.click(screen.getByRole("button", { name: "Model" }));
  await fireEvent.click(
    within(await screen.findByRole("list", { name: "Available models" })).getAllByRole("button")[0],
  );
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByLabelText("Profile ID")).not.toBeInTheDocument());
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      client_type: "claude-agent",
      client_key: "claude-agent",
    }),
  );
  await fireEvent.click(await screen.findByRole("button", { name: "Edit Executor" }));
  await fireEvent.click(screen.getByRole("button", { name: "Clear client and model" }));
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByLabelText("Profile ID")).not.toBeInTheDocument());
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      client_type: null,
      client_key: null,
      model: null,
    }),
  );
});

test("keeps entered values after a failed save so the user can correct and retry", async () => {
  const user = userEvent.setup();
  render(AgentProfilesPage);
  await user.click(screen.getByRole("button", { name: "New profile" }));
  await user.type(screen.getByLabelText("Profile ID"), "executor");
  await user.type(screen.getByLabelText("Name", { exact: true }), "Duplicate");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("alert");
  expect(screen.getByLabelText("Name", { exact: true })).toHaveValue("Duplicate");
  await waitFor(() => expect(screen.getByLabelText("Profile ID")).toBeEnabled());
  await user.clear(screen.getByLabelText("Profile ID"));
  await user.type(screen.getByLabelText("Profile ID"), "unique");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("button", { name: "Edit Duplicate" });
  expect(device.data.agentProfiles.find((p) => p.profile_id === "unique")?.name).toBe("Duplicate");
});

test("cancels deletion without changing data, then deletes after confirmation", async () => {
  render(AgentProfilesPage);
  await fireEvent.click(await screen.findByRole("button", { name: "Delete Executor" }));
  await fireEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancel" }),
  );
  expect(device.data.agentProfiles.some((p) => p.profile_id === "executor")).toBe(true);
  await fireEvent.click(screen.getByRole("button", { name: "Delete Executor" }));
  await fireEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete", exact: true }),
  );
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Edit Executor" })).not.toBeInTheDocument(),
  );
  expect(device.data.agentProfiles.some((p) => p.profile_id === "executor")).toBe(false);
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});

test("reads an individual profile and cannot read it after deletion", async () => {
  const profile = await testDevice.api.getAgentProfile("executor");
  expect(profile).toMatchObject({
    profile_id: "executor",
    name: "Executor",
    client_type: null,
    model: null,
  });
  await testDevice.api.deleteAgentProfile("executor");
  await expect(testDevice.api.getAgentProfile("executor")).rejects.toMatchObject({ status: 404 });
});

test("preserves an unavailable saved client until the user clears it", async () => {
  const user = userEvent.setup();
  Object.assign(device.data.agentProfiles[0], {
    client_type: "removed",
    client_key: "old",
    model: "old-model",
  });
  render(AgentProfilesPage);
  await user.click(await screen.findByRole("button", { name: "Edit Executor" }));
  await user.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("alert");
  expect(device.data.agentProfiles[0]).toMatchObject({
    client_type: "removed",
    model: "old-model",
  });
  await user.click(screen.getByRole("button", { name: "Clear client and model" }));
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(device.data.agentProfiles[0]).toMatchObject({
      client_type: null,
      client_key: null,
      model: null,
    }),
  );
});
