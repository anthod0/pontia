import { afterEach, expect, test, vi } from "vitest";
import { waitFor } from "@testing-library/svelte";
import { BotTimeline, compareBotEntries } from "../src/lib/botTimeline.svelte";
import type { BotEntry, BotMessage, BotMessagePage } from "../src/api/types";
import { ApiError } from "../src/api/errors";
import { testDevice } from "./deviceFixture";

function message(n: number): BotMessage {
  return {
    id: String(n),
    kind: "user_input",
    content: String(n),
    status: "normal",
    created_at: new Date(n).toISOString(),
  };
}

function page(entries: BotEntry[], before: string | null, session = "s"): BotMessagePage {
  return {
    bot: {
      bot_id: "bot",
      name: "Bot",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "test",
      reasoning_effort: null,
    },
    status: { kind: "idle" },
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: session,
    generation_notice: null,
    next_before: before,
    entries,
  };
}

afterEach(() => vi.restoreAllMocks());

test("loads overlapping history once and catches up across a missed tail while reconciling an older visible row", async () => {
  let total = 150;
  let failed = false;
  const fetch = vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      const end = before ? Number(before) : around ? Number(around) + 26 : total;
      const start = Math.max(0, end - 50);
      const entries = Array.from({ length: end - start }, (_, i) => message(start + i));
      const edited = entries.find((row) => row.id === "80");
      if (edited && failed) edited.status = "failed";
      return page(entries, start ? String(start) : null);
    });
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh(true);
  await waitFor(() => expect(timeline.entries).toHaveLength(50));
  timeline.loadOlder();
  timeline.loadOlder();
  await waitFor(() => expect(timeline.entries).toHaveLength(100));
  timeline.setVisible(["80"]);
  total = 276;
  failed = true;
  timeline.refresh(true);
  await waitFor(() =>
    expect(timeline.entries.find((row) => row.id === "80")).toMatchObject({ status: "failed" }),
  );
  expect(timeline.entries.map((row) => row.id)).toEqual(
    Array.from({ length: 226 }, (_, i) => String(i + 50)),
  );
  expect(fetch.mock.calls.filter(([, p]) => p.before === "100")).toHaveLength(1);
  timeline.message(message(80));
  expect(timeline.entries.find((row) => row.id === "80")).toMatchObject({ status: "normal" });
  timeline.destroy();
});

test("retries a failed history page at its original cursor, and refreshes a rotated generation", async () => {
  const notice = {
    id: "notice",
    kind: "session.exited" as const,
    created_at: message(3).created_at,
  };
  const fetch = vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockResolvedValueOnce({ ...page([message(2), notice], "old"), generation_notice: notice })
    .mockRejectedValueOnce(new Error("offline"))
    .mockRejectedValueOnce(new ApiError("rotated", "bot_timeline_reset", 409))
    .mockResolvedValueOnce(page([message(2), message(4)], "new", "s2"))
    .mockResolvedValueOnce(page([message(1), message(2)], null, "s2"));
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries).toHaveLength(2));
  timeline.loadOlder();
  await waitFor(() => expect(timeline.historyError).toBeTruthy());
  expect(timeline.before).toBe("old");
  expect(timeline.entries[1]).toEqual(notice);
  timeline.loadOlder();
  await waitFor(() => expect(timeline.entries.map((e) => e.id)).toEqual(["1", "2", "4"]));
  expect(timeline.before).toBeNull();
  expect(timeline.generationNotice).toBeNull();
  expect(fetch.mock.calls.filter(([, p]) => p.before === "old")).toHaveLength(2);
  timeline.destroy();
});

test("a newer stream update wins over an in-flight page; switching Bot ignores a late response", async () => {
  const fetch = vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockResolvedValueOnce(page([message(4)], null));
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries).toHaveLength(1));
  let finish!: (page: BotMessagePage) => void;
  fetch.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  timeline.refresh();
  timeline.message({ ...message(4), status: "failed" });
  finish(page([message(4)], null));
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3));
  expect(timeline.entries[0]).toMatchObject({ status: "failed" });
  timeline.destroy();
  finish(page([message(6)], null));
  await Promise.resolve();
  expect(timeline.entries.map((e) => e.id)).toEqual(["4"]);
});

test("orders mixed timestamps by UTC millisecond and binary ID", () => {
  const a = { ...message(1), id: "Z", created_at: "2026-11-20T10:00:00.123999+08:00" };
  const b = { ...message(2), id: "a", created_at: "2026-11-20T02:00:00.123Z" };
  expect([b, a].sort(compareBotEntries).map((row) => row.id)).toEqual(["Z", "a"]);
});

test("refreshes the chat and shared sidebar summary through processing, working, and idle", async () => {
  const fetch = vi.spyOn(testDevice.api, "getBotMessages");
  const timeline = new BotTimeline(testDevice, "bot");
  const statuses: BotMessagePage["status"][] = [
    { kind: "processing" },
    { kind: "working", title: "Review changes" },
    { kind: "idle" },
  ];
  for (const status of statuses) {
    fetch.mockResolvedValue({ ...page([], null), status, open_todo_count: 3 });
    timeline.refresh();
    await waitFor(() => expect(timeline.status).toEqual(status));
    expect(testDevice.queryClient.getQueryData(["bot-summary", "bot"])).toEqual({
      status,
      open_todo_count: 3,
    });
  }
  timeline.destroy();
});

function history(total: number) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      if (around !== undefined) {
        if (Number(around) >= total)
          throw new ApiError("Bot timeline entry not found", "not_found", 404);
        return page([message(Number(around))], null);
      }
      const end = before ? Number(before) : total;
      const start = Math.max(0, end - 50);
      return page(
        Array.from({ length: end - start }, (_, i) => message(start + i)),
        start ? String(start) : null,
      );
    });
}

test("reveals an old entry by loading every page back to it", async () => {
  history(300);
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries).toHaveLength(50));
  await expect(timeline.reveal("120")).resolves.toBe(true);
  expect(timeline.entries.map((row) => row.id)).toEqual(
    Array.from({ length: 200 }, (_, i) => String(i + 100)),
  );
  timeline.destroy();
});

test("reveals a loaded entry without fetching", async () => {
  const fetch = history(300);
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries).toHaveLength(50));
  const calls = fetch.mock.calls.length;
  await expect(timeline.reveal("280")).resolves.toBe(true);
  expect(fetch).toHaveBeenCalledTimes(calls);
  timeline.destroy();
});

test("reports an entry that does not exist without loading older history", async () => {
  history(300);
  const timeline = new BotTimeline(testDevice, "bot");
  timeline.refresh();
  await waitFor(() => expect(timeline.entries).toHaveLength(50));
  await expect(timeline.reveal("999")).resolves.toBe(false);
  expect(timeline.entries).toHaveLength(50);
  expect(timeline.before).toBe("250");
  timeline.destroy();
});

test.each(["unmarked", "closed"])(
  "refresh finds a new mark on a visible %s historical message",
  async (previous) => {
    const old = { ...message(1), kind: "bot_reply" as const };
    const closed = {
      ...old,
      todo:
        previous === "closed"
          ? { state: "done" as const, by: "user" as const, at: old.created_at }
          : undefined,
    };
    const reopened = {
      ...old,
      todo: { state: "open" as const, by: "bot" as const, at: "2026-11-21T08:00:00Z" },
    };
    const latest = page([message(2)], "older");
    vi.spyOn(testDevice.api, "getBotMessages")
      .mockResolvedValueOnce(latest)
      .mockResolvedValueOnce(page([closed], null))
      .mockResolvedValueOnce({ ...latest, open_todo_count: 1 })
      .mockResolvedValueOnce(page([reopened], null));
    const timeline = new BotTimeline(testDevice, "bot");
    timeline.refresh();
    await waitFor(() => expect(timeline.entries).toHaveLength(1));
    timeline.loadOlder();
    await waitFor(() => expect(timeline.entries[0]).toEqual(closed));
    timeline.setVisible([old.id]);
    timeline.refresh();
    await waitFor(() => expect(timeline.entries[0]).toEqual(reopened));
    expect(timeline.openTodoCount).toBe(1);
    timeline.destroy();
  },
);
