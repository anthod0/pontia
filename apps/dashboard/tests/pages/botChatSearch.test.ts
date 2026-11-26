import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotEntry, BotMessage, BotMessagePage } from "../../src/api/types";
import BotChatPreviewHarness from "./BotChatPreviewHarness.svelte";
import { VisibleObserver, mockFileInfo } from "../components/fileCardFixture";

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

function serve(
  entries: BotEntry[],
  older: BotEntry[] = [],
  fileLinkSession: BotMessagePage["file_link_session"] = null,
) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      const base = {
        bot,
        status: { kind: "idle" as const },
        file_link_session: fileLinkSession,
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
    { q: "部署", type: "all", before: undefined },
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
    { q: "部署", type: "all", before: answer.id },
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
  await testDevice.api.searchBotMessages(bot.bot_id, { q: "100% 部署", type: "all" });
  await testDevice.api.searchBotMessages(bot.bot_id, {
    q: "",
    type: "image",
    before: answer.id,
  });
  const urls = fetch.mock.calls.map(([input]) => new URL(String(input), "http://localhost"));
  expect(urls.map((url) => url.pathname)).toEqual([
    "/api/v1/bots/search/messages/search",
    "/api/v1/bots/search/messages/search",
  ]);
  expect(urls.map((url) => Object.fromEntries(url.searchParams))).toEqual([
    { q: "100% 部署", type: "all" },
    { q: "", type: "image", before: answer.id },
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

const uploads = '<user_uploads note="Temporary files; copy any you need into the workspace.">';
const shared = message(
  3,
  "bot_reply",
  "Done:\n\n```attachments\n/work/report.md\n/work/chart.png\n```",
);
const linked = message(
  4,
  "bot_reply",
  "See [the notes](/work/notes.md#L2) and ![plot](/work/plot.png)",
);
const uploaded = message(
  5,
  "user_input",
  `${uploads}\n<user_upload path="/up/1/photo.png">photo.png</user_upload>\n</user_uploads>\nLook`,
);
const attachment = (
  source: "user_upload" | "attachment_block" | "markdown_link",
  path: string,
) => ({
  source,
  media_kind: path.endsWith(".png") ? ("image" as const) : ("file" as const),
  path,
  name: path.slice(path.lastIndexOf("/") + 1),
});
const fileHits = [
  { ...linked, attachments: [attachment("markdown_link", "/work/notes.md")] },
  { ...shared, attachments: [attachment("attachment_block", "/work/report.md")] },
];
const imageHits = [
  { ...uploaded, attachments: [attachment("user_upload", "/up/1/photo.png")] },
  { ...linked, attachments: [attachment("markdown_link", "/work/plot.png")] },
  { ...shared, attachments: [attachment("attachment_block", "/work/chart.png")] },
];

function serveFiles() {
  return vi
    .spyOn(testDevice.api, "searchBotMessages")
    .mockImplementation(async (_bot, { q, type }) => ({
      messages: (type === "image" ? imageHits : type === "file" ? fileHits : []).filter((hit) =>
        hit.attachments.some((file) => file.path.includes(q)),
      ),
      next_before: null,
    }));
}

test("lists every file without search text and keeps the type while text and tabs change", async () => {
  serve([latest]);
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({ open: [], recently_resolved: [] });
  const search = serveFiles();
  const { panel, input } = await openSearch();
  expect(panel.getByText("Find messages from you and this Bot")).toBeVisible();
  await fireEvent.click(panel.getByRole("radio", { name: "Files" }));

  const results = await panel.findByRole("list", { name: "Search results" });
  expect(search).toHaveBeenLastCalledWith(
    bot.bot_id,
    { q: "", type: "file", before: undefined },
    expect.any(AbortSignal),
  );
  expect(
    within(results)
      .getAllByRole("button")
      .map((row) => row.dataset.searchFile),
  ).toEqual(["/work/notes.md", "/work/report.md"]);
  expect(within(results).getAllByRole("button")[0]).toHaveTextContent("Helper Bot");

  await fireEvent.input(input, { target: { value: "report" } });
  await waitFor(() =>
    expect(search).toHaveBeenLastCalledWith(
      bot.bot_id,
      { q: "report", type: "file", before: undefined },
      expect.any(AbortSignal),
    ),
  );
  await waitFor(() =>
    expect(
      [...panel.getByRole("list").querySelectorAll("mark")].map((mark) => mark.textContent),
    ).toEqual(["report", "report"]),
  );

  await fireEvent.click(panel.getByRole("button", { name: "Clear" }));
  expect(input).toHaveValue("");
  expect(panel.getByRole("radio", { name: "Files" })).toHaveAttribute("aria-checked", "true");
  await waitFor(() => expect(panel.getByRole("list").querySelectorAll("li")).toHaveLength(2));

  await fireEvent.click(screen.getByRole("tab", { name: "Todo" }));
  await screen.findByText("No todos");
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  expect(panel.getByRole("radio", { name: "Files" })).toHaveAttribute("aria-checked", "true");
});

test("previews reply images through the file Session but never uploads", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  mockFileInfo();
  serve([latest], [], { session_id: "s", workspace: "/work" });
  serveFiles();
  const preview = vi.spyOn(testDevice.previews, "imageUrl").mockResolvedValue("blob:image");
  const { panel } = await openSearch();
  await fireEvent.click(panel.getByRole("radio", { name: "Images" }));
  const results = await panel.findByRole("list", { name: "Search results" });
  expect(
    within(results)
      .getAllByRole("button")
      .map((row) => row.dataset.searchFile),
  ).toEqual(["/up/1/photo.png", "/work/plot.png", "/work/chart.png"]);
  await waitFor(() => expect(results.querySelectorAll("img")).toHaveLength(2));
  expect(preview.mock.calls.map(([, path]) => path).sort()).toEqual([
    "/work/chart.png",
    "/work/plot.png",
  ]);
  expect(preview.mock.calls.every(([session]) => session === "s")).toBe(true);
  vi.unstubAllGlobals();
});

test("does not download image search previews before they enter the viewport", async () => {
  class HiddenObserver {
    observe() {}
    disconnect() {}
  }
  vi.stubGlobal("IntersectionObserver", HiddenObserver);
  serve([latest], [], { session_id: "s", workspace: "/work" });
  const images = Array.from({ length: 20 }, (_, index) => {
    const path = `/work/image-${index}.png`;
    return {
      ...message(index + 10, "bot_reply", `![image](${path})`),
      attachments: [attachment("markdown_link", path)],
    };
  });
  vi.spyOn(testDevice.api, "searchBotMessages").mockResolvedValue({
    messages: images,
    next_before: null,
  });
  const info = vi.spyOn(testDevice.fileApi, "pathFileInfo");
  const preview = vi.spyOn(testDevice.previews, "imageUrl");
  const { panel } = await openSearch();

  await fireEvent.click(panel.getByRole("radio", { name: "Images" }));
  const results = await panel.findByRole("list", { name: "Search results" });

  expect(within(results).getAllByRole("button")).toHaveLength(20);
  expect(info).not.toHaveBeenCalled();
  expect(preview).not.toHaveBeenCalled();
  vi.unstubAllGlobals();
});

test("jumps to an attached file, or to the message a linked file is in", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  mockFileInfo();
  vi.spyOn(testDevice.previews, "imageUrl").mockResolvedValue(null);
  serve([shared, linked, latest], [], { session_id: "s", workspace: "/work" });
  serveFiles();
  const { panel } = await openSearch();
  await fireEvent.click(panel.getByRole("radio", { name: "Files" }));
  const result = async (path: string) =>
    (await screen.findByRole("list", { name: "Search results" })).querySelector<HTMLElement>(
      `[data-search-file="${path}"]`,
    )!;

  await fireEvent.click(await result("/work/notes.md"));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      linked.id,
    ),
  );

  // Choosing a result closes the narrow panel; it reopens on the search with its type.
  await fireEvent.click(screen.getByRole("button", { name: "Sidebar" }));
  await fireEvent.click(await result("/work/report.md"));
  await waitFor(() =>
    expect(document.querySelector('[data-shared-file="/work/report.md"]')!.className).toContain(
      "ring-2",
    ),
  );
  expect(document.querySelector("[data-highlighted]")).toBeNull();
  await waitFor(() => expect(scroll).toHaveBeenCalled());
  delete (Element.prototype as Partial<Element>).scrollIntoView;
  vi.unstubAllGlobals();
});
