import { mocks } from "./fixtures";
import { testDevice } from "../../deviceFixture";
import { render, screen, within, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { test, vi, expect } from "vitest";
import NewChatPage from "../../../src/pages/NewChatPage.svelte";
vi.mock("$lib/hooks/shellMode.svelte", () => ({ isMobileShell: () => true }));
vi.mock("../../../src/device/hostContext", () => ({
  useDashboardHost: () => ({ activeDevice: { name: "Local device" } }),
}));
test("keeps per-client picks within the sheet and submits only the confirmed selection", async () => {
  const user = userEvent.setup();
  const catalog = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [
      {
        id: "model-a",
        name: "Model A",
        description: "",
        reasoning_efforts: [{ id: "high", name: "High", description: "" }],
      },
    ],
    version: null,
    fetched_at: Date.now(),
    default_model: "model-a",
  });
  try {
    render(NewChatPage);
    const open = await screen.findByRole("button", { name: /Pi/ });
    await user.click(open);
    const sheet = within(await screen.findByRole("dialog"));
    await user.click(await sheet.findByRole("button", { name: "Model A" }));
    await user.click(sheet.getByRole("radio", { name: "High" }));
    await user.click(sheet.getByRole("radio", { name: "codex" }));
    expect(sheet.queryByRole("searchbox")).toBeNull();
    await user.click(sheet.getByRole("radio", { name: "Pi" }));
    expect(sheet.getByRole("radio", { name: "High" })).toHaveAttribute("aria-checked", "true");
    await user.click(sheet.getByRole("button", { name: "Done" }));
    await user.click(open);
    const reopened = within(await screen.findByRole("dialog"));
    await user.click(reopened.getByRole("radio", { name: "codex" }));
    await user.click(reopened.getByRole("button", { name: "Close" }));
    await user.type(await screen.findByPlaceholderText("Describe a task…"), "Review changes");
    await user.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(mocks.createSession).toHaveBeenCalledWith(
        expect.objectContaining({ client_type: "pi", model: "model-a", reasoning_effort: "high" }),
      ),
    );
  } finally {
    catalog.mockRestore();
  }
});

test("keeps a rejected prompt in place and retries from the inline error", async () => {
  const user = userEvent.setup();
  mocks.createSession.mockRejectedValueOnce(new Error("Device is offline"));
  render(NewChatPage);

  const editor = await screen.findByPlaceholderText("Describe a task…");
  await user.type(editor, "Review changes");
  await user.click(screen.getByRole("button", { name: "Send" }));

  const alert = await screen.findByRole("alert");
  expect(alert).toBeVisible();
  expect(editor).toHaveTextContent("Review changes");

  await user.click(within(alert).getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledTimes(2));
});
