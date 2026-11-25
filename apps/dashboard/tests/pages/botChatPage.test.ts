import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotMessagePage } from "../../src/api/types";
import { VisibleObserver, mockFileInfo } from "../components/fileCardFixture";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import BotChatPreviewHarness from "./BotChatPreviewHarness.svelte";
import type { BotUpdate } from "../../src/services/botChatStream";
import ShortcutManager from "../../src/lib/components/shortcuts/ShortcutManager.svelte";

const stream = vi.hoisted(() => ({ push: (_update: BotUpdate) => {} }));
vi.mock("../../src/services/botChatStream", () => ({
  openBotChatStream: (_device: unknown, _id: string, onUpdate: (update: BotUpdate) => void) => {
    stream.push = onUpdate;
    return () => {};
  },
}));

const bot: Bot = {
  bot_id: "a",
  name: "Bot",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
  reasoning_effort: null,
};

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 640 : 56;
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollTo");
});

test("renders Bot replies as markdown while keeping user messages as plain text", async () => {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [
      {
        id: "msg-user",
        kind: "user_input",
        content: "Keep [literal](https://example.com/user) raw",
        status: "normal",
        created_at: "2026-11-20T00:00:00Z",
      },
      {
        id: "msg-bot",
        kind: "bot_reply",
        content: [
          "See [docs](https://example.com/docs).",
          "",
          "```ts",
          "const answer = 42;",
          "```",
          "",
          '<script>alert("unsafe")</script>',
        ].join("\n"),
        status: "normal",
        created_at: "2026-11-20T00:00:01Z",
      },
    ],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });

  const { container } = render(BotChatPage, { botId: bot.bot_id });

  const link = await screen.findByRole("link", { name: /docs/i });
  await waitFor(() => {
    expect(link).toHaveAttribute("href", "https://example.com/docs");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
  await waitFor(() => {
    expect(container.querySelector("[data-code-block] code")).toHaveTextContent(
      "const answer = 42;",
    );
  });
  expect(container.querySelector("script")).not.toBeInTheDocument();

  expect(screen.queryByRole("link", { name: /literal/i })).not.toBeInTheDocument();
  expect(screen.getByText("Keep [literal](https://example.com/user) raw")).toBeInTheDocument();
});

test("shows the send time of every message and notice in the feed", async () => {
  const times = [
    "2026-11-20T00:00:00.000Z",
    "2026-11-20T00:00:01.000Z",
    "2026-11-20T00:00:02.000Z",
  ];
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [
      { id: "u", kind: "user_input", content: "Hi", status: "failed", created_at: times[0] },
      { id: "r", kind: "bot_reply", content: "Hello", status: "normal", created_at: times[1] },
      { id: "n", kind: "session.exited", created_at: times[2] },
    ],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  render(BotChatPage, { botId: bot.bot_id });
  const log = await screen.findByRole("log");
  await waitFor(() => expect(log.querySelectorAll("[data-bot-entry-id]")).toHaveLength(3));
  ["u", "r", "n"].forEach((id, index) => {
    const row = log.querySelector(`[data-bot-entry-id="${id}"]`)!;
    expect(row.querySelector("time")).toHaveAttribute("datetime", times[index]);
  });
});

test.each([
  ["/tmp/report.md", "/tmp/report.md"],
  ["reports/report.md", "/bot/a/reports/report.md"],
])("opens %s from a historical Bot reply in the file preview panel", async (href, path) => {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [
      {
        id: "reply",
        kind: "bot_reply",
        content: `[查看报告](${href})`,
        status: "normal",
        created_at: "2026-11-20T00:00:00Z",
      },
    ],
    file_link_session: { session_id: "previous", workspace: "/bot/a" },
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    next_before: null,
    status: { kind: "idle" },
  });
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("# Report"));
  render(BotChatPreviewHarness, { botId: bot.bot_id });

  await fireEvent.click(await screen.findByRole("button", { name: "Preview report.md" }));
  await within(screen.getByRole("complementary")).findByRole("heading", { name: "Report" });
  expect(read).toHaveBeenCalledWith("previous", path, expect.any(AbortSignal));
});

function chatWith(processing: boolean, count = 1, workingOn: string | null = null) {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: Array.from({ length: count }, (_, index) => ({
      id: `msg-${index}`,
      kind: "user_input",
      content: index === 0 ? "Hello" : `Message ${index}`,
      status: "normal",
      created_at: new Date(index).toISOString(),
    })),
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: processing
      ? { kind: "processing" }
      : workingOn
        ? { kind: "working", title: workingOn }
        : { kind: "idle" },
  });
  render(BotChatPage, { botId: bot.bot_id });
  return screen.findByRole("log");
}

test("mounts only the visible window of a full Bot history page", async () => {
  const log = await chatWith(false, 50);
  await waitFor(() => {
    const rows = log.querySelectorAll("[data-bot-entry-id]");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(30);
  });
});

test("a live Bot reply shows its attachments as a card and downloads them", async () => {
  const latest: BotMessagePage = {
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  };
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue(latest);
  const download = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("workbook", { headers: { "Content-Length": "8" } }));
  const save = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("log");
  const reply = replies(["The workbook is ready.", "/work/Pontia项目描述.xlsx"])[0];
  latest.entries = [reply];
  latest.file_link_session = { session_id: "main", workspace: "/work" };
  stream.push({ type: "message", message: reply });
  expect(await screen.findByText("The workbook is ready.")).toBeInTheDocument();
  await fireEvent.click(
    await screen.findByRole("button", { name: "Download Pontia项目描述.xlsx" }),
  );
  expect(download).toHaveBeenCalledWith(
    "main",
    "/work/Pontia项目描述.xlsx",
    expect.any(AbortSignal),
  );
  await waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.instances[0]).toHaveProperty("download", "Pontia项目描述.xlsx");
});

/** Bot replies, each with its text and an attachments block of the given paths. */
function replies(...messages: [string, ...string[]][]) {
  return messages.map(([text, ...paths], index) => ({
    id: `bmsg-${index}`,
    kind: "bot_reply" as const,
    content: [text, "", "```attachments", ...paths, "```"].join("\n"),
    status: "normal" as const,
    created_at: `2026-11-20T00:00:0${index}Z`,
  }));
}

function attachments(...messages: string[][]) {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: replies(
      ...messages.map((names, i): [string, ...string[]] => [
        `Reply ${i}`,
        ...names.map((name) => `/work/${name}`),
      ]),
    ),
    file_link_session: { session_id: "main", workspace: "/work" },
    open_todo_count: 0,
    notice_session_id: "main",
    generation_notice: null,
    next_before: null,
    status: { kind: "idle" },
  });
}

test("shows an attached image inline and enlarges it in the image viewer", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:bot-image");
  mockFileInfo();
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("png", { headers: { "Content-Type": "image/png" } }));
  attachments(["chart.png"]);
  render(BotChatPreviewHarness, { botId: bot.bot_id });

  const enlarge = await screen.findByRole("button", { name: "Enlarge chart.png" });
  expect(within(enlarge).getByRole("img", { name: "chart.png" })).toHaveAttribute(
    "src",
    "blob:bot-image",
  );
  expect(read).toHaveBeenCalledWith("main", "/work/chart.png");
  expect(screen.getByText("Reply 0")).toBeInTheDocument();
  await fireEvent.click(enlarge);
  const viewer = await screen.findByRole("dialog");
  expect(within(viewer).queryByRole("button", { name: "Next image" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download chart.png" })).toBeInTheDocument();
});

test("pages the image viewer only within one message's attachments", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  mockFileInfo();
  vi.spyOn(testDevice.previews, "imageUrl").mockImplementation(async (_s, path) => `blob:${path}`);
  attachments(["before.png"], ["a.png", "b.png"]);
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Enlarge a.png" }));
  const viewer = await screen.findByRole("dialog");
  expect(within(viewer).getByRole("button", { name: "Previous image" })).toBeDisabled();
  await fireEvent.click(within(viewer).getByRole("button", { name: "Next image" }));
  expect(within(viewer).getByRole("heading", { name: "b.png", hidden: true })).toBeInTheDocument();
  expect(within(viewer).getByRole("button", { name: "Next image" })).toBeDisabled();
});

test("opens an attached text file in the file preview panel and leaves binary files download-only", async () => {
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("# Notes\n"));
  attachments(["notes.md", "bundle.zip"]);
  render(BotChatPreviewHarness, { botId: bot.bot_id });

  expect(await screen.findByRole("button", { name: "Download bundle.zip" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Preview bundle.zip" })).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  await within(screen.getByRole("complementary")).findByRole("heading", { name: "Notes" });
  expect(read).toHaveBeenCalledWith("main", "/work/notes.md", expect.any(AbortSignal));
});

test("a Bot reply with an invalid attachments block shows it as a code block", async () => {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: replies(["See below.", "/work/a.txt", "b.txt"]),
    file_link_session: { session_id: "main", workspace: "/work" },
    open_todo_count: 0,
    notice_session_id: "main",
    generation_notice: null,
    next_before: null,
    status: { kind: "idle" },
  });
  render(BotChatPage, { botId: bot.bot_id });
  expect(await screen.findByText(/b\.txt/)).toBeInTheDocument();
  expect(document.querySelector("[data-shared-file]")).not.toBeInTheDocument();
});

test("shows message bubble placeholders until the first page of messages arrives", async () => {
  let resolve!: (page: BotMessagePage) => void;
  vi.spyOn(testDevice.api, "getBotMessages").mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  render(BotChatPage, { botId: bot.bot_id });
  const skeleton = screen.getByRole("status", { name: "Loading conversation" });
  expect(skeleton).toHaveAttribute("data-bot-conversation-skeleton");
  const bubbles = [...skeleton.querySelectorAll(".rounded-2xl")];
  expect(bubbles.map((bubble) => bubble.parentElement?.className)).toEqual([
    "flex justify-end",
    "flex justify-start",
  ]);
  expect(bubbles.every((bubble) => bubble.childElementCount === 0)).toBe(true);
  expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  resolve({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  await screen.findByRole("log", { name: "Messages" });
  expect(screen.queryByRole("status", { name: "Loading conversation" })).not.toBeInTheDocument();
});

class BottomObserver {
  static instances: BottomObserver[] = [];
  target: Element | null = null;
  constructor(
    private readonly callback: IntersectionObserverCallback,
    readonly options?: IntersectionObserverInit,
  ) {
    BottomObserver.instances.push(this);
  }
  observe(target: Element) {
    this.target = target;
  }
  disconnect() {}
  trigger(isIntersecting: boolean) {
    this.callback(
      [{ isIntersecting, target: this.target } as IntersectionObserverEntry],
      this as never,
    );
  }
}

async function feedBottom(log: HTMLElement) {
  await waitFor(() =>
    expect(BottomObserver.instances.some((item) => item.options?.root === log)).toBe(true),
  );
  return BottomObserver.instances.findLast((item) => item.options?.root === log)!;
}

test("shows a scroll-down button away from the bottom of the Bot feed and follows to the end when clicked", async () => {
  BottomObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", BottomObserver);
  const scrollTo = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollTo", { configurable: true, value: scrollTo });
  vi.spyOn(Element.prototype, "scrollHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 4000 : 56;
  });
  const log = await chatWith(false, 50);
  const bottom = await feedBottom(log);

  bottom.trigger(false);
  const button = await screen.findByRole("button", { name: /scroll to bottom/i });
  scrollTo.mockClear();
  await fireEvent.click(button);

  expect(scrollTo).toHaveBeenCalledWith(expect.objectContaining({ top: 4000 - 640 }));
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: /scroll to bottom/i })).not.toBeInTheDocument(),
  );
});

test("shows that an answered Bot is still working until its session becomes idle", async () => {
  const latest: BotMessagePage = {
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "busy" },
  };
  const fetch = vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue(latest);
  render(BotChatPage, { botId: bot.bot_id });

  expect(await screen.findByText("Still working")).toBeInTheDocument();
  fetch.mockResolvedValue({ ...latest, status: { kind: "idle" } });
  stream.push({ type: "changed" });
  await waitFor(() => expect(screen.queryByText("Still working")).not.toBeInTheDocument());
});

test("hides the scroll-down button at the bottom of the Bot feed", async () => {
  BottomObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", BottomObserver);
  const log = await chatWith(false, 3);
  const bottom = await feedBottom(log);

  bottom.trigger(false);
  await screen.findByRole("button", { name: /scroll to bottom/i });
  bottom.trigger(true);

  await waitFor(() =>
    expect(screen.queryByRole("button", { name: /scroll to bottom/i })).not.toBeInTheDocument(),
  );
});

test("shows the files opened before in an empty Files tab and reopens one from the list", async () => {
  localStorage.clear();
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockImplementation(async (_session, path) => new Response(`# ${path.slice(6, -3)}\n`));
  attachments(["notes.md", "plan.md"]);
  const first = render(BotChatPreviewHarness, { botId: bot.bot_id });

  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  await fireEvent.click(screen.getByRole("tab", { name: "Files" }));
  expect(await screen.findByText("No open file")).toBeVisible();

  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByRole("heading", { name: "notes" });
  await fireEvent.click(screen.getByRole("button", { name: "Preview plan.md" }));
  await within(panel).findByRole("heading", { name: "plan" });
  await fireEvent.click(within(panel).getByRole("button", { name: "Close file" }));

  // After a reload the list is still there, newest first.
  first.unmount();
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  await fireEvent.click(screen.getByRole("tab", { name: "Files" }));
  const list = await screen.findByRole("region", { name: "Recently opened" });
  expect(
    within(list)
      .getAllByRole("button", { name: /^Preview/ })
      .map((button) => button.getAttribute("title")),
  ).toEqual(["/work/plan.md", "/work/notes.md"]);

  read.mockClear();
  await fireEvent.click(within(list).getByRole("button", { name: "Preview notes.md" }));
  await within(screen.getByRole("complementary")).findByRole("heading", { name: "notes" });
  expect(read).toHaveBeenCalledWith("main", "/work/notes.md", expect.any(AbortSignal));

  await fireEvent.click(screen.getByRole("button", { name: "Close file" }));
  await fireEvent.click(await screen.findByRole("button", { name: "Clear list" }));
  expect(await screen.findByText("No open file")).toBeVisible();
});

test("the sidebar button shows the open todo count and reopens the tab last used on the page", async () => {
  localStorage.clear();
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({ open: [], recently_resolved: [] });
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 3,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  const first = render(BotChatPreviewHarness, { botId: bot.bot_id });
  const button = await screen.findByRole("button", { name: "Sidebar, 3 todos" });
  expect(button).toHaveTextContent("3");
  expect(button).toHaveAttribute("aria-pressed", "false");

  await fireEvent.click(button);
  expect(button).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("tab", { name: "Files" })).toHaveAttribute("aria-selected", "true");
  await fireEvent.click(screen.getByRole("tab", { name: /Todo/ }));
  await screen.findByText("No todos");
  await fireEvent.click(button);
  await waitFor(() => expect(screen.queryByRole("complementary")).not.toBeInTheDocument());
  await fireEvent.click(button);
  expect(screen.getByRole("tab", { name: /Todo/ })).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByText("No todos")).toBeVisible();
  await fireEvent.click(button);
  await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));

  // Another Bot chat page starts on Files.
  first.unmount();
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar, 3 todos" }));
  expect(screen.getByRole("tab", { name: "Files" })).toHaveAttribute("aria-selected", "true");
});

test("Bot shortcuts move between Bots in directory order and wrap", async () => {
  const bots = [bot, { ...bot, bot_id: "b", name: "Second" }];
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue(bots);
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  history.replaceState({}, "", "/bots/a");
  render(ShortcutManager);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("log");
  await waitFor(() => expect(testDevice.api.listBots).toHaveBeenCalled());

  await fireEvent.keyDown(window, { code: "KeyB", altKey: true });
  await waitFor(() => expect(location.pathname).toBe("/dashboard/bots/b"));
  history.replaceState({}, "", "/bots/a");
  await fireEvent.keyDown(window, { code: "KeyB", altKey: true, shiftKey: true });
  await waitFor(() => expect(location.pathname).toBe("/dashboard/bots/b"));
});

test("Bot panel shortcut opens the panel and cycles its tabs in both directions", async () => {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  history.replaceState({}, "", "/bots/a");
  render(ShortcutManager);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("log");

  await fireEvent.keyDown(window, { code: "KeyP", altKey: true });
  await waitFor(() =>
    expect(screen.getByRole("tab", { name: "Preview" })).toHaveAttribute("aria-selected", "true"),
  );
  await fireEvent.keyDown(window, { code: "KeyP", altKey: true, shiftKey: true });
  await waitFor(() =>
    expect(screen.getByRole("tab", { name: "Files" })).toHaveAttribute("aria-selected", "true"),
  );
});

function emptyBotChat() {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
}

test("on wide screens the URL keeps the side panel and its tab without adding history", async () => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(min-width: 1024px)",
    addEventListener() {},
    removeEventListener() {},
  }));
  emptyBotChat();
  const entries = history.length;
  const first = render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeVisible();
  expect(location.search).toBe("?panel=files");
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  expect(location.search).toBe("?panel=search");
  expect(history.length).toBe(entries);
  expect(localStorage.length).toBe(0);

  // A reload of the same URL reopens the panel on its tab.
  first.unmount();
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await screen.findByRole("complementary", { name: "Sidebar" });
  expect(within(panel).getByRole("tab", { name: "Search" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await fireEvent.click(within(panel).getByRole("button", { name: "Close" }));
  expect(location.search).toBe("");
  expect(history.length).toBe(entries);
  expect(screen.queryByRole("complementary", { name: "Sidebar" })).not.toBeInTheDocument();
});

test("an unknown tab in the URL opens the side panel on Files", async () => {
  emptyBotChat();
  history.replaceState({}, "", "?panel=unknown");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await screen.findByRole("complementary", { name: "Sidebar" });
  expect(within(panel).getByRole("tab", { name: "Files" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("on narrower screens Back closes the side panel and closing it goes back", async () => {
  emptyBotChat();
  const url = location.href;
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const button = await screen.findByRole("button", { name: "Sidebar" });
  expect(screen.queryByRole("complementary", { name: "Sidebar" })).not.toBeInTheDocument();

  await fireEvent.click(button);
  expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeVisible();
  expect(location.search).toBe("?panel=files");
  // Switching tabs replaces the entry the panel added, so one Back closes it.
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  expect(location.search).toBe("?panel=search");
  history.back();
  await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));
  expect(location.href).toBe(url);

  history.forward();
  await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));
  expect(screen.getByRole("tab", { name: "Search" })).toHaveAttribute("aria-selected", "true");
  await fireEvent.click(screen.getByRole("button", { name: "Close" }));
  await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));
  expect(location.href).toBe(url);
  expect(localStorage.length).toBe(0);
});
