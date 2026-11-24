import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import { waitFor } from "@testing-library/svelte";
import { createBotDirectory } from "../src/features/bots/botDirectory.svelte";
import { testDevice } from "./deviceFixture";

test("removing the only Bot with open todos clears the directory indicator", async () => {
  const cache = testDevice.queryClient;
  const bots = ["first", "second"].map((bot_id) => ({ bot_id }));
  cache.setQueryData(["bots"], bots);
  cache.setQueryData(["bot-summary", "first"], { open_todo_count: 0 });
  cache.setQueryData(["bot-summary", "second"], { open_todo_count: 1 });
  const pending = vi
    .spyOn(testDevice.api, "listBots")
    .mockImplementation(() => new Promise(() => {}));
  const summaries = vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(() => new Promise(() => {}));
  let directory!: ReturnType<typeof createBotDirectory>;
  const stop = $effect.root(() => {
    directory = createBotDirectory(testDevice);
  });
  try {
    flushSync();
    await waitFor(() => expect(directory.hasOpenTodos).toBe(true));
    cache.setQueryData(["bots"], bots.slice(0, 1));
    await waitFor(() => expect(directory.hasOpenTodos).toBe(false));
  } finally {
    stop();
    pending.mockRestore();
    summaries.mockRestore();
  }
});
