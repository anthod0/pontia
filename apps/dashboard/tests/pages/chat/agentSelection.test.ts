import { mocks, session, turn, workspace } from "./fixtures";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { testDevice } from "../../deviceFixture";

const NewChatPage = (await import("../../../src/pages/NewChatPage.svelte")).default;

test("submits the selected configured agent's opaque metadata through ordinary Session creation", async () => {
  const user = userEvent.setup();
  const metadata = { acp_agent_id: "company:代理", custom: { preserved: true } };
  mocks.listSessionCreationOptions.mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", metadata: {} },
    { id: "opaque-choice", name: "Company Agent", client_type: "acp", metadata },
  ]);
  mocks.createSession.mockResolvedValue({ session: session(), initial_turn: turn() });
  render(NewChatPage);
  await user.click(await screen.findByLabelText("Agent client"));
  await user.click(await screen.findByText("Company Agent"));
  document.body.style.pointerEvents = "";
  await user.type(screen.getByRole("textbox"), "Review the changes");
  await user.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() =>
    expect(mocks.createSession).toHaveBeenCalledWith(
      expect.objectContaining({
        client_type: "acp",
        metadata: { ...metadata, source: "dashboard_chat", title_source: "input" },
        initial_task: { input: "Review the changes", metadata: { source: "dashboard_chat" } },
      }),
    ),
  );
});

test("prevents creation while discovery is unavailable and recovers after an explicit retry", async () => {
  const user = userEvent.setup();
  mocks.listSessionCreationOptions.mockRejectedValueOnce(new Error("Discovery unavailable"));
  mocks.createSession.mockResolvedValue({ session: session(), initial_turn: turn() });
  render(NewChatPage);
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalled());
  await user.type(screen.getByRole("textbox"), "Review the changes");
  expect(screen.getByRole("button", { name: "Start session" })).toBeDisabled();
  expect(mocks.createSession).not.toHaveBeenCalled();
  const notification = mocks.toastError.mock.lastCall?.[1];
  notification.action.onClick();
  await waitFor(() => expect(screen.getByRole("button", { name: "Start session" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() =>
    expect(mocks.createSession).toHaveBeenCalledWith(
      expect.objectContaining({ client_type: "pi" }),
    ),
  );
});

test("Alt+C and Alt+W switch the agent client and workspace, with Shift going back", async () => {
  mocks.listSessionCreationOptions.mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", metadata: {}, version: "0.70.2" },
    { id: "company", name: "Company Agent", client_type: "acp", metadata: {}, version: "1.2.3" },
  ]);
  mocks.workspaces.set([
    workspace({ workspace_id: "ws-1", name: "first" }),
    workspace({ workspace_id: "ws-2", name: "second" }),
  ]);
  render(NewChatPage);
  const clientSelect = await screen.findByLabelText("Agent client");
  await waitFor(() => expect(clientSelect).toHaveTextContent("Pi"));
  expect(clientSelect).not.toHaveTextContent("0.70.2");

  await fireEvent.keyDown(window, { key: "a", code: "KeyA", altKey: true });
  expect(clientSelect).toHaveTextContent("Pi");
  await fireEvent.keyDown(window, { key: "c", code: "KeyC", altKey: true });
  expect(clientSelect).toHaveTextContent("Company Agent");
  expect(clientSelect).not.toHaveTextContent("1.2.3");
  await fireEvent.keyDown(window, { key: "C", code: "KeyC", altKey: true, shiftKey: true });
  expect(clientSelect).toHaveTextContent("Pi");

  const workspaceSelect = screen.getByLabelText("Workspace", { exact: true });
  await waitFor(() => expect(workspaceSelect).toHaveTextContent("first"));
  await fireEvent.keyDown(window, { key: "w", code: "KeyW", altKey: true });
  expect(workspaceSelect).toHaveTextContent("second");
  await fireEvent.keyDown(window, { key: "w", code: "KeyW", altKey: true });
  expect(workspaceSelect).toHaveTextContent("first");
});

test.each([false, true])(
  "selects a workspace model and resets it when the workspace changes: %s",
  async (switchWorkspace) => {
    const models = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
      version: null,
      fetched_at: Date.now(),
      default_model: "fast",
      models: [
        { id: "fast", name: "Fast", description: "" },
        { id: "deep", name: "Deep", description: "" },
      ],
    });
    mocks.listSessionCreationOptions.mockResolvedValue([
      {
        id: "company",
        name: "Company Agent",
        client_type: "acp",
        model_catalog_key: "company-id",
        version: null,
        metadata: { acp_agent_id: "company-id" },
      },
    ]);
    mocks.workspaces.set([
      workspace({ workspace_id: "ws-1", name: "first" }),
      workspace({ workspace_id: "ws-2", name: "second" }),
    ]);
    mocks.createSession.mockResolvedValue({ session: session(), initial_turn: turn() });
    const user = userEvent.setup();
    render(NewChatPage);
    await user.click(await screen.findByRole("button", { name: "Fast" }));
    await user.click(await screen.findByLabelText("Deep"));
    document.body.style.pointerEvents = "";
    await waitFor(() =>
      expect(models).toHaveBeenCalledWith(
        "acp",
        "company-id",
        expect.any(AbortSignal),
        "ws-1",
        true,
      ),
    );
    if (switchWorkspace) {
      await fireEvent.keyDown(window, { key: "w", code: "KeyW", altKey: true });
    }
    await user.type(screen.getByPlaceholderText("Describe a task…"), "Review the changes");
    await user.click(screen.getByRole("button", { name: "Start session" }));
    await waitFor(() => expect(mocks.createSession).toHaveBeenCalled());
    const request = mocks.createSession.mock.lastCall?.[0];
    expect(request.workspace_id).toBe(switchWorkspace ? "ws-2" : "ws-1");
    expect(request.model).toBe(switchWorkspace ? undefined : "deep");
    models.mockRestore();
  },
);
