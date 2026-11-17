import { mocks, session, turn, workspace } from "./fixtures";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";

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

test("Alt+A and Alt+W switch the agent client and workspace, with Shift going back", async () => {
  mocks.listSessionCreationOptions.mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", metadata: {} },
    { id: "company", name: "Company Agent", client_type: "acp", metadata: {} },
  ]);
  mocks.workspaces.set([
    workspace({ workspace_id: "ws-1", name: "first" }),
    workspace({ workspace_id: "ws-2", name: "second" }),
  ]);
  render(NewChatPage);
  const clientSelect = await screen.findByLabelText("Agent client");
  await waitFor(() => expect(clientSelect).toHaveTextContent("Pi"));

  await fireEvent.keyDown(window, { key: "a", code: "KeyA", altKey: true });
  expect(clientSelect).toHaveTextContent("Company Agent");
  await fireEvent.keyDown(window, { key: "A", code: "KeyA", altKey: true, shiftKey: true });
  expect(clientSelect).toHaveTextContent("Pi");

  const workspaceSelect = screen.getByLabelText("Workspace", { exact: true });
  await waitFor(() => expect(workspaceSelect).toHaveTextContent("first"));
  await fireEvent.keyDown(window, { key: "w", code: "KeyW", altKey: true });
  expect(workspaceSelect).toHaveTextContent("second");
  await fireEvent.keyDown(window, { key: "w", code: "KeyW", altKey: true });
  expect(workspaceSelect).toHaveTextContent("first");
});
