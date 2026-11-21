import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotMessagePage, SharedFile } from "../../src/api/types";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import BotChatPreviewHarness from "./BotChatPreviewHarness.svelte";
import type { BotUpdate } from "../../src/services/botChatStream";

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
};

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 640 : 56;
  });
});
afterEach(() => vi.restoreAllMocks());

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

test("downloads a live file share directly from the open Bot chat", async () => {
  const file = {
    file_id: "file-1",
    turn_id: null,
    name: "Pontia项目描述.xlsx",
    path: "/work/Pontia项目描述.xlsx",
    size: 5821,
    media_type: "application/octet-stream",
    created_at: "2026-11-20T00:00:00Z",
  };
  const latest: BotMessagePage = {
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  };
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue(latest);
  const events = vi.spyOn(testDevice.events, "subscribeDashboardEvents");
  const download = vi
    .spyOn(testDevice.fileApi, "sharedFileContent")
    .mockResolvedValue(new Response("workbook", { headers: { "Content-Length": "8" } }));
  const save = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("log");
  latest.notice_session_id = "main";
  latest.entries = [
    {
      kind: "shared_file",
      id: file.file_id,
      session_id: "main",
      created_at: file.created_at,
      file,
    },
  ];
  events.mock.calls[0][0]({
    kind: "session_event",
    id: "e",
    occurred_at: file.created_at,
    event: {
      event_id: "e",
      session_id: "main",
      turn_id: null,
      source: "external_api",
      type: "session.file_shared",
      time: file.created_at,
      payload: { file_id: file.file_id },
      file,
    },
  });
  await fireEvent.click(
    await screen.findByRole("button", { name: "Download Pontia项目描述.xlsx" }),
  );
  expect(download).toHaveBeenCalledWith("main", "file-1", expect.any(AbortSignal));
  await waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.instances[0]).toHaveProperty("download", file.name);
});

function sharedFiles(...files: SharedFile[]) {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: files.map((file) => ({
      kind: "shared_file",
      id: file.file_id,
      session_id: "main",
      created_at: file.created_at,
      file,
    })),
    file_link_session: null,
    notice_session_id: "main",
    generation_notice: null,
    next_before: null,
    status: { kind: "idle" },
  });
}

const sharedFile = (name: string, media_type = "application/octet-stream"): SharedFile => ({
  file_id: `file-${name}`,
  turn_id: null,
  name,
  path: `/work/${name}`,
  size: 12,
  media_type,
  created_at: "2026-11-20T00:00:00Z",
});

test("shows a shared image inline and enlarges it in the image viewer", async () => {
  class VisibleObserver {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe(target: Element) {
      this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
    }
    disconnect() {}
  }
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:bot-image");
  const read = vi
    .spyOn(testDevice.fileApi, "sharedFileContent")
    .mockResolvedValue(new Response("png", { headers: { "Content-Type": "image/png" } }));
  sharedFiles(sharedFile("chart.png", "image/png"));
  render(BotChatPreviewHarness, { botId: bot.bot_id });

  const enlarge = await screen.findByRole("button", { name: "Enlarge chart.png" });
  expect(within(enlarge).getByRole("img", { name: "chart.png" })).toHaveAttribute(
    "src",
    "blob:bot-image",
  );
  expect(read).toHaveBeenCalledWith("main", "file-chart.png");
  await fireEvent.click(enlarge);
  expect(await screen.findByRole("dialog")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download chart.png" })).toBeInTheDocument();
  vi.unstubAllGlobals();
});

test("opens a shared text file in the file preview panel and leaves binary files download-only", async () => {
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("# Notes\n"));
  sharedFiles(sharedFile("notes.md"), sharedFile("bundle.zip"));
  render(BotChatPreviewHarness, { botId: bot.bot_id });

  expect(await screen.findByRole("button", { name: "Download bundle.zip" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Preview bundle.zip" })).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  await within(screen.getByRole("complementary")).findByRole("heading", { name: "Notes" });
  expect(read).toHaveBeenCalledWith("main", "/work/notes.md", expect.any(AbortSignal));
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
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  await screen.findByRole("log", { name: "Messages" });
  expect(screen.queryByRole("status", { name: "Loading conversation" })).not.toBeInTheDocument();
});
