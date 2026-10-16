import { mocks, session, turn } from "./fixtures";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";

const NewChatPage = (await import("../../../src/pages/NewChatPage.svelte")).default;

test("submits the selected configured agent's opaque metadata through ordinary Session creation", async () => {
  const user = userEvent.setup();
  const metadata = { acp_agent_id: "company:代理", custom: { preserved: true } };
  mocks.listSessionCreationOptions.mockResolvedValue([
    { id: "pi", name: "pi", client_type: "pi", metadata: {} },
    { id: "opaque-choice", name: "Company Agent", client_type: "acp", metadata },
  ]);
  mocks.createSession.mockResolvedValue({ session: session(), initial_turn: turn() });
  render(NewChatPage);
  await user.click(await screen.findByRole("button", { name: "Company Agent" }));
  await user.type(screen.getByRole("textbox"), "Review the changes");
  await user.click(screen.getByRole("button", { name: "Start session" }));
  await waitFor(() =>
    expect(mocks.createSession).toHaveBeenCalledWith(
      expect.objectContaining({
        client_type: "acp",
        metadata: { ...metadata, source: "dashboard_chat" },
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
