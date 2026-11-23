import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotEntry, BotMessage } from "../../src/api/types";
import BotChatPreviewHarness from "./BotChatPreviewHarness.svelte";

vi.mock("../../src/services/botChatStream", () => ({ openBotChatStream: () => () => {} }));

const bot: Bot = {
  bot_id: "search",
  name: "Helper Bot",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
  reasoning_effort: null,
};
const id = (n: number) => `bmsg_0193a000-0000-7000-8000-${String(n).padStart(12, "0")}`;
const message = (n: number, kind: BotMessage["kind"], content: string): BotMessage => ({
  id: id(n),
  kind,
  content,
  status: "normal",
  created_at: new Date(Date.UTC(2026, 10, 20, 9, n)).toISOString(),
});
const old = message(1, "user_input", "部署失败了，帮我看看日志");
const answer = message(2, "bot_reply", "部署日志显示**磁盘已满**");
const latest = message(9, "user_input", "Thanks");

function serve(entries: BotEntry[], older: BotEntry[] = []) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      const base = {
        bot,
        status: { kind: "idle" as const },
        file_link_session: null,
        open_todo_count: 0,
        notice_session_id: null,
        generation_notice: null,
      };
      if (around !== undefined) return { ...base, entries: [], next_before: null };
      if (before) return { ...base, entries: older, next_before: null };
      return { ...base, entries, next_before: older.length ? "older" : null };
    });
}

async function openSearch() {
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  const panel = screen.getByRole("complementary");
  const input = within(panel).getByRole("searchbox", { name: "Search messages" });
  return { panel: within(panel), input };
}

beforeEach(() => {
  localStorage.clear();
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 2000 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
});
afterEach(() => {
  vi.restoreAllMocks();
  testDevice.queryClient.clear();
});

test("searches this Bot's messages and marks the matched text in each result", async () => {
  serve([latest], [old, answer]);
  const search = vi
    .spyOn(testDevice.api, "searchBotMessages")
    .mockResolvedValue({ messages: [answer, old], next_before: null });
  const { panel, input } = await openSearch();
  expect(input).toHaveFocus();
  await fireEvent.input(input, { target: { value: " 部署 " } });

  const results = await panel.findByRole("list", { name: "Search results" });
  expect(search).toHaveBeenCalledWith(
    bot.bot_id,
    { q: "部署", before: undefined },
    expect.any(AbortSignal),
  );
  const [first, second] = within(results).getAllByRole("button");
  expect(first).toHaveTextContent("Helper Bot");
  expect(first).toHaveTextContent("部署日志显示磁盘已满");
  expect(first.querySelector("time")).toHaveAttribute("datetime", answer.created_at);
  expect(second).toHaveTextContent("You");
  expect([...results.querySelectorAll("mark")].map((mark) => mark.textContent)).toEqual([
    "部署",
    "部署",
  ]);
});

test("loads older results after the last one", async () => {
  serve([latest]);
  const search = vi
    .spyOn(testDevice.api, "searchBotMessages")
    .mockImplementation(async (_bot, { before }) =>
      before
        ? { messages: [old], next_before: null }
        : { messages: [answer], next_before: answer.id },
    );
  const { panel, input } = await openSearch();
  await fireEvent.input(input, { target: { value: "部署" } });
  await fireEvent.click(await panel.findByRole("button", { name: "Load more" }));
  await waitFor(() =>
    expect(panel.getByRole("list", { name: "Search results" }).querySelectorAll("li")).toHaveLength(
      2,
    ),
  );
  expect(search).toHaveBeenLastCalledWith(
    bot.bot_id,
    { q: "部署", before: answer.id },
    expect.any(AbortSignal),
  );
  expect(panel.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
});

test("says when nothing matches", async () => {
  serve([latest]);
  vi.spyOn(testDevice.api, "searchBotMessages").mockResolvedValue({
    messages: [],
    next_before: null,
  });
  const { panel, input } = await openSearch();
  await fireEvent.input(input, { target: { value: "missing" } });
  expect(await panel.findByText("No matching messages")).toBeVisible();
});

test("jumps to a result outside the loaded history and highlights it", async () => {
  const pages = serve([latest], [old, answer]);
  vi.spyOn(testDevice.api, "searchBotMessages").mockResolvedValue({
    messages: [old],
    next_before: null,
  });
  const { panel, input } = await openSearch();
  await fireEvent.input(input, { target: { value: "部署" } });
  await fireEvent.click(await panel.findByRole("button", { name: /部署失败了/ }));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      old.id,
    ),
  );
  expect(pages.mock.calls.some(([, page]) => page.before === "older")).toBe(true);
});

test("requests the first page without a cursor and later pages with one", async () => {
  const fetch = vi.fn(
    async () =>
      new Response(JSON.stringify({ data: { messages: [], next_before: null } }), {
        headers: { "Content-Type": "application/json" },
      }),
  );
  vi.stubGlobal("fetch", fetch);
  await testDevice.api.searchBotMessages(bot.bot_id, { q: "100% 部署" });
  await testDevice.api.searchBotMessages(bot.bot_id, { q: "部署", before: answer.id });
  const urls = fetch.mock.calls.map(([input]) => new URL(String(input), "http://localhost"));
  expect(urls.map((url) => url.pathname)).toEqual([
    "/api/v1/bots/search/messages/search",
    "/api/v1/bots/search/messages/search",
  ]);
  expect(urls.map((url) => Object.fromEntries(url.searchParams))).toEqual([
    { q: "100% 部署" },
    { q: "部署", before: answer.id },
  ]);
  vi.unstubAllGlobals();
});

test("marks only the current search text after the query changes", async () => {
  serve([latest]);
  let release = () => {};
  vi.spyOn(testDevice.api, "searchBotMessages").mockImplementation(async (_bot, { q }) => {
    if (q === "磁盘") await new Promise<void>((resolve) => (release = resolve));
    return { messages: q === "磁盘" ? [answer] : [old], next_before: null };
  });
  const { panel, input } = await openSearch();
  await fireEvent.input(input, { target: { value: "帮我" } });
  await panel.findByText("帮我");
  await fireEvent.input(input, { target: { value: "磁盘" } });
  await waitFor(() => expect(panel.queryByText("帮我")).not.toBeInTheDocument());
  release();
  expect((await panel.findByText("磁盘")).tagName).toBe("MARK");
});

test("focuses the search field when switching to it from another tab", async () => {
  serve([latest]);
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({ open: [], recently_resolved: [] });
  const search = vi.spyOn(testDevice.api, "searchBotMessages");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  await fireEvent.click(screen.getByRole("tab", { name: "Todo" }));
  await screen.findByText("No todos");
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  await waitFor(() =>
    expect(screen.getByRole("searchbox", { name: "Search messages" })).toHaveFocus(),
  );
  expect(search).not.toHaveBeenCalled();
});
