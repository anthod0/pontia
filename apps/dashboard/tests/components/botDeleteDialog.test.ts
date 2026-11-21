import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, SessionView } from "../../src/api/types";
import BotDeleteDialog from "../../src/components/bots/BotDeleteDialog.svelte";
import BotDeleteFromListHarness from "./BotDeleteFromListHarness.svelte";

const goto = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("$app/navigation", () => ({ goto }));

const bot = (bot_id: string): Bot => ({
  bot_id,
  name: bot_id,
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "provider/model",
});
const helper = { session_id: "helper-1", client_type: "pi", title: "Fix the build" } as SessionView;

beforeEach(() => {
  goto.mockClear();
  testDevice.queryClient.clear();
  window.history.replaceState({}, "", "/dashboard/bots/doomed");
});
afterEach(() => vi.restoreAllMocks());

test("open helpers keep the Bot from being deleted and can be opened from the dialog", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot("doomed"), bot("kept")]);
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: false,
    helpers: [helper],
  });
  const remove = vi.spyOn(testDevice.api, "deleteBot");
  const ondone = vi.fn();
  render(BotDeleteDialog, { bot: bot("doomed"), onclose: vi.fn(), ondone });
  const open = await screen.findByRole("button", { name: "Fix the build" });
  expect(screen.getByRole("button", { name: "Delete Bot" })).toBeDisabled();
  await fireEvent.click(open);
  expect(goto).toHaveBeenCalledWith("/dashboard/chat/helper-1", {});
  expect(ondone).toHaveBeenCalled();
  expect(remove).not.toHaveBeenCalled();
});

test("a working Bot cannot be deleted", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot("doomed")]);
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: true,
    helpers: [],
  });
  render(BotDeleteDialog, { bot: bot("doomed"), onclose: vi.fn(), ondone: vi.fn() });
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Delete Bot" })).toBeDisabled();
});

test("deleting the Bot opens the Bot entry", async () => {
  testDevice.queryClient.setQueryData(["bots"], [bot("doomed"), bot("kept")]);
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot("kept")]);
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: false,
    helpers: [],
  });
  const remove = vi.spyOn(testDevice.api, "deleteBot").mockResolvedValue();
  const ondone = vi.fn();
  render(BotDeleteDialog, { bot: bot("doomed"), onclose: vi.fn(), ondone });
  const button = screen.getByRole("button", { name: "Delete Bot" });
  await waitFor(() => expect(button).toBeEnabled());
  await fireEvent.click(button);
  await waitFor(() => expect(goto).toHaveBeenCalledWith("/dashboard/bot", { replaceState: true }));
  expect(remove).toHaveBeenCalledWith("doomed");
  expect(ondone).toHaveBeenCalled();
  expect(testDevice.queryClient.getQueryData<Bot[]>(["bots"])?.map((item) => item.bot_id)).toEqual([
    "kept",
  ]);
});

test("deleting from a view that only shows listed Bots still opens the Bot entry", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot("doomed"), bot("kept")]);
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: false,
    helpers: [],
  });
  vi.spyOn(testDevice.api, "deleteBot").mockResolvedValue();
  const ondone = vi.fn();
  render(BotDeleteFromListHarness, { botId: "doomed", ondone });
  const button = await screen.findByRole("button", { name: "Delete Bot" });
  await waitFor(() => expect(button).toBeEnabled());
  await fireEvent.click(button);
  await waitFor(() => expect(goto).toHaveBeenCalledWith("/dashboard/bot", { replaceState: true }));
  expect(ondone).toHaveBeenCalled();
});

test("a rejected deletion keeps the dialog open with the reason", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([bot("doomed")]);
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: false,
    helpers: [],
  });
  vi.spyOn(testDevice.api, "deleteBot").mockRejectedValue(new Error("Cannot delete this Bot"));
  const ondone = vi.fn();
  render(BotDeleteDialog, { bot: bot("doomed"), onclose: vi.fn(), ondone });
  const button = screen.getByRole("button", { name: "Delete Bot" });
  await waitFor(() => expect(button).toBeEnabled());
  await fireEvent.click(button);
  await screen.findByRole("alert");
  expect(ondone).not.toHaveBeenCalled();
  expect(goto).not.toHaveBeenCalled();
});
