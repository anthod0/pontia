import { createBotTodos } from "../src/features/bots/botTodos.svelte";
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

test("loads todos concurrently for just the Bots with open items", async () => {
  const ids = ["first", "empty", "last"];
  testDevice.queryClient.setQueryData(
    ["bots"],
    ids.map((bot_id) => ({ bot_id })),
  );
  for (const id of ids)
    testDevice.queryClient.setQueryData(["bot-summary", id], {
      open_todo_count: id === "empty" ? 0 : 2,
    });
  const getTodos = vi
    .spyOn(testDevice.api, "getBotTodos")
    .mockImplementation(() => new Promise(() => {}));
  const stop = $effect.root(() => {
    createBotTodos(testDevice, createBotDirectory(testDevice));
  });
  try {
    await waitFor(() => expect(getTodos.mock.calls.map(([id]) => id)).toEqual(["first", "last"]));
  } finally {
    stop();
    getTodos.mockRestore();
  }
});

test("loads recent history for a Bot with no open todos or cached history", async () => {
  const cache = testDevice.queryClient;
  cache.setQueryData(["bots"], [{ bot_id: "last" }]);
  cache.setQueryData(["bot-summary", "last"], { open_todo_count: 0 });
  const at = "2026-11-21T08:00:00Z";
  const done = {
    id: "handled",
    kind: "bot_reply",
    content: "Review complete",
    status: "normal",
    created_at: at,
    todo: { state: "done", by: "user", at },
  } as const;
  const request = vi
    .spyOn(testDevice.api, "getBotTodos")
    .mockResolvedValue({ open: [], recently_resolved: [done] });
  let todos!: ReturnType<typeof createBotTodos>;
  const stop = $effect.root(() => {
    todos = createBotTodos(testDevice, createBotDirectory(testDevice), true);
  });
  try {
    await waitFor(() => expect(todos.groups[0]?.result.data?.recently_resolved).toEqual([done]));
  } finally {
    stop();
    request.mockRestore();
  }
});
