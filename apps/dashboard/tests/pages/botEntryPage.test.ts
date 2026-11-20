import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import BotEntryPage from "../../src/pages/BotEntryPage.svelte";
import type { Bot } from "../../src/api/types";

const goto = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("$app/navigation", () => ({ goto }));

const first: Bot = {
  bot_id: "first",
  name: "Pontia",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "provider/model",
};

beforeEach(() => {
  goto.mockClear();
  window.history.replaceState({}, "", "/dashboard/bot");
  vi.spyOn(testDevice.api, "listBotClients").mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", client_key: "default" },
  ]);
  vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [{ id: "provider/model", name: "Test model", description: "provider" }],
    default_model: null,
  });
});
afterEach(() => vi.restoreAllMocks());

test("opens the first existing Bot without creating one", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([first, { ...first, bot_id: "second" }]);
  const save = vi.spyOn(testDevice.api, "saveBot");
  render(BotEntryPage);
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/dashboard/bots/first", { replaceState: true }),
  );
  expect(save).not.toHaveBeenCalled();
});

test("creates the first Bot from client and model and opens it on the selected device", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([]);
  const save = vi
    .spyOn(testDevice.api, "saveBot")
    .mockRejectedValueOnce(new Error("Offline"))
    .mockResolvedValue(first);
  render(BotEntryPage, { routeScope: "/office-mac" });
  await fireEvent.click(await screen.findByRole("radio", { name: "Mochi" }));
  await fireEvent.click(screen.getByRole("radio", { name: "Rose" }));
  await waitFor(() => expect(screen.getByLabelText("Model")).toBeEnabled());
  await fireEvent.click(screen.getByLabelText("Model"));
  await fireEvent.click(await screen.findByRole("button", { name: "Test model" }));
  const submit = screen.getByRole("button", { name: "Start chatting" });
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  await screen.findByRole("alert");
  expect(goto).not.toHaveBeenCalled();
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/office-mac/bots/first", { replaceState: true }),
  );
  expect(save).toHaveBeenLastCalledWith(
    {
      name: "Pontia",
      description: null,
      client_type: "pi",
      client_key: "default",
      model: "provider/model",
      avatar_shape: "mochi",
      avatar_color: "rose",
    },
    undefined,
  );
});

test("retries a failed Bot list before deciding whether to show setup", async () => {
  vi.spyOn(testDevice.api, "listBots")
    .mockRejectedValueOnce(new Error("Offline"))
    .mockResolvedValue([first]);
  testDevice.queryClient.setQueryDefaults(["bots"], { retry: false });
  render(BotEntryPage);
  await screen.findByRole("alert");
  expect(screen.queryByLabelText("Model")).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() =>
    expect(goto).toHaveBeenCalledWith("/dashboard/bots/first", { replaceState: true }),
  );
});

test.each([undefined, "/office-mac"])(
  "opens workspace setup in the same device scope: %s",
  async (routeScope) => {
    vi.spyOn(testDevice.api, "listBots").mockResolvedValue([]);
    render(BotEntryPage, { routeScope });
    const link = await screen.findByRole("link", {
      name: "Set up a workspace and start a Session",
    });
    const target = routeScope ?? "/dashboard";
    expect(link).toHaveAttribute("href", target);
    await fireEvent.click(link);
    await waitFor(() => expect(goto).toHaveBeenCalledWith(target, {}));
  },
);
