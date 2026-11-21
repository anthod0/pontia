import { afterEach, expect, test, vi } from "vitest";
import { waitFor } from "@testing-library/svelte";
import { BotTimeline } from "../src/lib/botTimeline.svelte";
import {
  readCachedBotTimeline,
  removeCachedBotTimeline,
  writeCachedBotTimeline,
} from "../src/lib/botTimelineCache";
import type { Bot, BotEntry, BotMessage, BotMessagePage } from "../src/api/types";
import { ApiError } from "../src/api/errors";
import { testDevice } from "./deviceFixture";

const bot = (bot_id: string, name = "Bot"): Bot => ({
  bot_id,
  name,
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
});

function message(n: number, content = String(n)): BotMessage {
  return {
    id: String(n),
    kind: "user_input",
    content,
    status: "normal",
    created_at: new Date(n).toISOString(),
  };
}

function page(id: string, entries: BotEntry[], before: string | null = null): BotMessagePage {
  return {
    bot: bot(id),
    status: { kind: "idle" },
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: "s",
    generation_notice: null,
    next_before: before,
    entries,
  };
}

function cache(id: string, entries: BotEntry[], name = "Cached") {
  return writeCachedBotTimeline({
    deviceId: testDevice.id,
    botId: id,
    bot: bot(id, name),
    fileLinkSession: null,
    entries,
  });
}

async function cachedIds(id: string) {
  return (await readCachedBotTimeline(testDevice.id, id))?.entries.map((row) => row.id);
}

afterEach(() => vi.restoreAllMocks());

test("shows cached entries before the network answers, then replaces them with the latest page", async () => {
  await cache("first-paint", [message(1), message(2, "old text"), message(3)]);
  let answer!: (page: BotMessagePage) => void;
  vi.spyOn(testDevice.api, "getBotMessages").mockImplementation(
    () => new Promise((resolve) => (answer = resolve)),
  );
  const timeline = new BotTimeline(testDevice, "first-paint");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries.map((row) => row.id)).toEqual(["1", "2", "3"]));
  expect(timeline.bot?.name).toBe("Cached");
  // Row 3 was removed and row 2 edited on the server since the cache was written.
  answer(page("first-paint", [message(1), message(2, "new text"), message(4)], "1"));
  await waitFor(() => expect(timeline.entries.map((row) => row.id)).toEqual(["1", "2", "4"]));
  expect(timeline.entries[1]).toMatchObject({ content: "new text" });
  expect(timeline.bot?.name).toBe("Bot");
  expect(timeline.before).toBe("1");
  await waitFor(async () => expect(await cachedIds("first-paint")).toEqual(["1", "2", "4"]));
  timeline.destroy();
});

test("keeps cached entries visible when the network is unavailable, without overwriting the cache", async () => {
  await cache("offline", [message(1)]);
  vi.spyOn(testDevice.api, "getBotMessages").mockRejectedValue(new Error("offline"));
  const timeline = new BotTimeline(testDevice, "offline");
  await waitFor(() => expect(timeline.entries.map((row) => row.id)).toEqual(["1"]));
  timeline.refresh();
  await waitFor(() => expect(timeline.refreshError).toBe("offline"));
  expect(timeline.entries.map((row) => row.id)).toEqual(["1"]);
  expect((await readCachedBotTimeline(testDevice.id, "offline"))?.bot.name).toBe("Cached");
  timeline.destroy();
});

test("writes live messages back and keeps only the latest page", async () => {
  let latest = 50;
  vi.spyOn(testDevice.api, "getBotMessages").mockImplementation(async (_id, { before }) =>
    before
      ? page("write-back", [message(0)])
      : page(
          "write-back",
          Array.from({ length: 50 }, (_, i) => message(latest - 49 + i)),
          "older",
        ),
  );
  const timeline = new BotTimeline(testDevice, "write-back");
  timeline.refresh();
  await waitFor(() => expect(timeline.before).toBe("older"));
  timeline.loadOlder();
  await waitFor(() => expect(timeline.entries).toHaveLength(51));
  latest = 51;
  timeline.message(message(51));
  await waitFor(async () => expect((await cachedIds("write-back"))?.at(-1)).toBe("51"));
  const ids = await cachedIds("write-back");
  expect(ids).toHaveLength(50);
  expect(ids?.[0]).toBe("2");
  timeline.destroy();
});

test("a deleted Bot clears its cache instead of showing cached entries", async () => {
  await cache("gone", [message(1)]);
  vi.spyOn(testDevice.api, "getBotMessages").mockRejectedValue(
    new ApiError("Bot gone not found", "not_found", 404),
  );
  const timeline = new BotTimeline(testDevice, "gone");
  await waitFor(() => expect(timeline.bot).not.toBeNull());
  timeline.refresh();
  await waitFor(() => expect(timeline.bot).toBeNull());
  expect(timeline.entries).toEqual([]);
  await waitFor(async () => expect(await readCachedBotTimeline(testDevice.id, "gone")).toBeNull());
  timeline.destroy();
});

test("keeps caches per device and Bot, and removes one without touching the others", async () => {
  await cache("kept", [message(1)]);
  await cache("removed", [message(2)]);
  await writeCachedBotTimeline({
    deviceId: "other-device",
    botId: "removed",
    bot: bot("removed"),
    fileLinkSession: null,
    entries: [message(3)],
  });
  await removeCachedBotTimeline(testDevice.id, "removed");
  expect(await readCachedBotTimeline(testDevice.id, "removed")).toBeNull();
  expect(await cachedIds("kept")).toEqual(["1"]);
  expect((await readCachedBotTimeline("other-device", "removed"))?.entries).toEqual([message(3)]);
});

test("discards a snapshot written by another cache version", async () => {
  await cache("versioned", [message(1)]);
  const snapshot = await readCachedBotTimeline(testDevice.id, "versioned");
  const database = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open("pontia-device-bot-timelines");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  const transaction = database.transaction("bot-timelines", "readwrite");
  const store = transaction.objectStore("bot-timelines");
  store.put({ ...snapshot, version: 0 });
  await new Promise((resolve) => (transaction.oncomplete = resolve));
  database.close();
  expect(await readCachedBotTimeline(testDevice.id, "versioned")).toBeNull();
});
