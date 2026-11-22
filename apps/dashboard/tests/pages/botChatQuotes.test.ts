import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEventBase from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotEntry, BotMessage } from "../../src/api/types";
import { ApiError } from "../../src/api/errors";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import { saveUploadRecord } from "../../src/lib/uploads/records";

const toast = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  dismiss: vi.fn(),
}));
vi.mock("svelte-sonner", () => ({ toast }));
vi.mock("../../src/services/botChatStream", () => ({ openBotChatStream: () => () => {} }));

const bot: Bot = {
  bot_id: "quotes",
  name: "Helper Bot",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
};
const id = (n: number) => `bmsg_0193a000-0000-7000-8000-${String(n).padStart(12, "0")}`;
const fileId = "file_0193a000-0000-7000-8000-000000000001";
const imageId = "file_0193a000-0000-7000-8000-000000000002";
const sharedFile = (file_id: string, name: string, media_type: string) => ({
  file_id,
  share_id: "share_0193a000-0000-7000-8000-000000000001",
  turn_id: null,
  name,
  path: `/work/${name}`,
  size: 12,
  media_type,
  created_at: new Date(2000).toISOString(),
});
const message = (n: number, kind: BotMessage["kind"], content: string): BotMessage => ({
  id: id(n),
  kind,
  content,
  status: "normal",
  created_at: new Date(n * 1000).toISOString(),
});
const reply = message(1, "bot_reply", "Renew the **certificate** first.");
const file: BotEntry = {
  kind: "shared_files",
  id: "share_0193a000-0000-7000-8000-000000000001",
  session_id: "s",
  created_at: new Date(2000).toISOString(),
  files: [
    sharedFile(fileId, "checklist.md", "text/markdown"),
    sharedFile(imageId, "chart.png", "image/png"),
  ],
};

function serve(entries: BotEntry[], options: { older?: BotEntry[]; missing?: string[] } = {}) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      if (around !== undefined && options.missing?.includes(around))
        throw new ApiError("Bot timeline entry not found", "not_found", 404);
      const base = {
        bot,
        status: { kind: "idle" as const },
        file_link_session: null,
        open_todo_count: 0,
        notice_session_id: "s",
        generation_notice: null,
      };
      if (around !== undefined) return { ...base, entries: [], next_before: null };
      if (before) return { ...base, entries: options.older ?? [], next_before: null };
      return { ...base, entries, next_before: options.older ? "older" : null };
    });
}

function sent() {
  return vi
    .spyOn(testDevice.api, "sendBotMessage")
    .mockImplementation(async (_bot, content) => message(99, "user_input", content));
}

// A message's menu offers Reply; a file's offers Quote file or Quote image.
async function quoteFromMenu(row: HTMLElement) {
  await fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
  await fireEvent.click(
    await screen.findByRole("menuitem", { name: /^(Reply|Quote file|Quote image)/ }),
  );
}

// A tap reaches the button as a click whose pointer is a touch.
async function tap(element: Element) {
  await fireEvent(
    element,
    new PointerEvent("click", { bubbles: true, cancelable: true, pointerType: "touch" }),
  );
}

// The closing menu briefly leaves pointer-events: none on the body in jsdom.
const userEvent = userEventBase.setup({ pointerEventsCheck: 0 });
const rows = () => screen.getAllByRole("article");

beforeEach(() => {
  localStorage.clear();
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 2000 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
  // jsdom has no scrollIntoView; file cards scroll a quoted file into view.
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
  vi.restoreAllMocks();
  toast.error.mockReset();
});

test("sends a quoted Bot reply as a user_quote element before the typed text", async () => {
  serve([reply]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "Who owns this?");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote message_id="${id(1)}" speaker="Bot">Renew the **certificate** first.</user_quote>\nWho owns this?`,
    ),
  );
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: /^Remove quote/ })).not.toBeInTheDocument(),
  );
});

test("quotes one file of a share with its ID and path", async () => {
  serve([file]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText(/checklist/);
  expect(rows()).toHaveLength(2);
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "Check it");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote file_id="${fileId}" speaker="Bot" path="/work/checklist.md">checklist.md</user_quote>\nCheck it`,
    ),
  );
});

test("a second quote replaces the first", async () => {
  serve([reply, message(2, "bot_reply", "Second answer")]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("Second answer");
  await quoteFromMenu(rows()[0]);
  await quoteFromMenu(rows()[1]);
  expect(screen.getAllByRole("button", { name: /^Remove quote/ })).toHaveLength(1);
  await userEvent.type(screen.getByRole("textbox"), "ok");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0][1]).toBe(
    `<user_quote message_id="${id(2)}" speaker="Bot">Second answer</user_quote>\nok`,
  );
});

test("keeps the pending quote after reopening the chat until it is removed", async () => {
  serve([reply]);
  const first = render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  first.unmount();
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Remove quote/ }));
  expect(screen.queryByRole("button", { name: /^Remove quote/ })).not.toBeInTheDocument();
});

test("cannot send a quote without text, even with an attachment", async () => {
  const scope = `quotes:${crypto.randomUUID()}`;
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue(scope);
  await saveUploadRecord({
    scope,
    draft: `bot:${bot.bot_id}`,
    key: crypto.randomUUID(),
    uploadId: "u1",
    name: "a.txt",
    size: 1,
    sha256: "a".repeat(64),
  });
  vi.spyOn(testDevice.uploads, "uploadRequest").mockResolvedValue({
    upload_id: "u1",
    name: "a.txt",
    size: 1,
    sha256: "a".repeat(64),
    offset: 1,
    completed: true,
    path: "/device/a.txt",
    created_at: "2026-11-18T00:00:00Z",
  });
  serve([reply]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  const submit = await screen.findByRole("button", { name: "Send" });
  await waitFor(() => expect(submit).toBeEnabled());
  await quoteFromMenu(rows()[0]);
  expect(submit).toBeDisabled();
  await userEvent.keyboard("{Enter}");
  expect(send).not.toHaveBeenCalled();
  await userEvent.type(screen.getByRole("textbox"), "x");
  await fireEvent.click(submit);
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0][1]).toMatch(
    /^<user_quote message_id=[^]*<\/user_quote>\n<user_uploads [^]*<\/user_uploads>\nx$/,
  );
});

test("keeps the quote when sending fails", async () => {
  serve([reply]);
  vi.spyOn(testDevice.api, "sendBotMessage").mockRejectedValue(new Error("Offline"));
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "hi");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Offline", expect.anything()));
  expect(screen.getByRole("button", { name: /^Remove quote/ })).toBeInTheDocument();
});

test("copies the visible text of a user message", async () => {
  serve([
    message(
      3,
      "user_input",
      `<user_quote message_id="${id(1)}" speaker="Bot">x</user_quote>\nMy question`,
    ),
  ]);
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("My question");
  await fireEvent.contextMenu(rows()[0], { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Copy" }));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith("My question"));
});

test("keeps the browser menu for Shift+right-click and links", async () => {
  serve([message(1, "bot_reply", "See [docs](https://example.com).")]);
  render(BotChatPage, { botId: bot.bot_id });
  const link = await screen.findByRole("link", { name: /docs/ });
  expect(await fireEvent.contextMenu(link)).toBe(true);
  expect(await fireEvent.contextMenu(rows()[0], { shiftKey: true })).toBe(true);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

test("jumps to a loaded quoted message and highlights it", async () => {
  serve([
    reply,
    message(
      2,
      "user_input",
      `<user_quote message_id="${id(1)}" speaker="Bot">Renew</user_quote>\nOK`,
    ),
  ]);
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Quote from Helper Bot/ }));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      id(1),
    ),
  );
});

test("loads older history to reach a quoted message", async () => {
  const older = message(1, "user_input", "An old question");
  const fetch = serve(
    [
      message(
        5,
        "user_input",
        `<user_quote message_id="${id(1)}" speaker="User">An old question</user_quote>\nAgain`,
      ),
    ],
    { older: [older] },
  );
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Quote from You/ }));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      id(1),
    ),
  );
  expect(fetch).toHaveBeenCalledWith(
    bot.bot_id,
    expect.objectContaining({ before: "older" }),
    expect.anything(),
  );
});

test("reports a quoted message that no longer exists and stops offering the jump", async () => {
  serve(
    [
      message(
        5,
        "user_input",
        `<user_quote message_id="${id(1)}" speaker="Bot">gone</user_quote>\nStill?`,
      ),
    ],
    { missing: [id(1)] },
  );
  render(BotChatPage, { botId: bot.bot_id });
  const jump = await screen.findByRole("button", { name: /^Quote from Helper Bot/ });
  await fireEvent.click(jump);
  await waitFor(() => expect(toast.error).toHaveBeenCalledOnce());
  expect(jump).toBeDisabled();
  expect(document.querySelector("[data-highlighted]")).toBeNull();
});

test("opens the message menu from the keyboard and quotes with R", async () => {
  serve([reply]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  const row = rows()[0];
  row.focus();
  await userEvent.keyboard("r");
  expect(screen.getByRole("button", { name: /^Remove quote/ })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole("textbox")).toHaveFocus());
  row.focus();
  await userEvent.keyboard("{Shift>}{F10}{/Shift}");
  expect(within(await screen.findByRole("menu")).getAllByRole("menuitem").length).toBeGreaterThan(
    0,
  );
});

test("quotes one image from its menu, shows its thumbnail, and jumps back to it", async () => {
  class VisibleObserver {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe(target: Element) {
      this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
    }
    disconnect() {}
  }
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockResolvedValue("blob:chart");
  const quoted = message(
    3,
    "user_input",
    `<user_quote file_id="${imageId}" speaker="Bot" path="/work/chart.png">chart.png</user_quote>\nBigger`,
  );
  serve([file, quoted]);
  render(BotChatPage, { botId: bot.bot_id });
  const image = await screen.findByRole("img", { name: "chart.png" });
  await fireEvent.contextMenu(image, { clientX: 10, clientY: 10 });
  const menu = await screen.findByRole("menu");
  expect(within(menu).getByText("chart.png")).toBeInTheDocument();
  expect(within(menu).getByRole("menuitem", { name: "Download" })).toBeInTheDocument();
  await fireEvent.click(within(menu).getByRole("menuitem", { name: /Quote image/ }));
  const pending = document.querySelector("[data-bot-pending-quote]")!;
  expect(pending.querySelector("[data-quote-thumbnail]")).toHaveAttribute("src", "blob:chart");

  const jump = screen.getByRole("button", { name: /^Quote from Helper Bot: chart\.png/ });
  expect(jump.querySelector("[data-quote-thumbnail]")).toBeInTheDocument();
  await fireEvent.click(jump);
  // A mixed share is a list; the quoted row is ringed.
  await waitFor(() =>
    expect(document.querySelector(`[data-shared-file="${imageId}"]`)!.className).toContain(
      "ring-2",
    ),
  );
  vi.unstubAllGlobals();
});

test("a touch long press keeps the native text selection instead of opening the menu", async () => {
  serve([reply]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await fireEvent.pointerDown(rows()[0], { pointerType: "touch" });
  expect(await fireEvent.contextMenu(rows()[0])).toBe(true);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("the message actions button opens the touch menu to reply to the whole message", async () => {
  serve([reply, file]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  const [replyMenu, fileMenu] = screen.getAllByRole("button", { name: "Message actions" });
  await tap(fileMenu);
  const sheet = within(await screen.findByRole("dialog"));
  await fireEvent.click(sheet.getByRole("menuitem", { name: /Quote file/ }));
  await screen.findByRole("button", { name: /^Remove quote.*checklist/ });
  await tap(replyMenu);
  const replySheet = within(await screen.findByRole("dialog"));
  await fireEvent.click(replySheet.getByRole("menuitem", { name: "Reply" }));
  await screen.findByRole("button", { name: /^Remove quote.*certificate/ });
});

test("with a mouse, the message actions button opens the same menu as a right-click", async () => {
  serve([reply]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  const button = screen.getByRole("button", { name: "Message actions" });
  expect(button).toHaveClass("opacity-0", "group-hover/entry:opacity-100");
  expect(button).not.toHaveAttribute("aria-expanded");
  await userEvent.click(button);
  const menu = await screen.findByRole("menu");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent),
  ).toEqual(["ReplyR", "Copy", "View context"]);
  // The button stays visible while its menu is open.
  expect(button).toHaveAttribute("aria-expanded", "true");
  await userEvent.keyboard("{Escape}");
  await waitFor(() => expect(button).toHaveFocus());
  expect(button).not.toHaveAttribute("aria-expanded");
  await userEvent.click(button);
  await fireEvent.click(await screen.findByRole("menuitem", { name: /Reply/ }));
  await screen.findByRole("button", { name: /^Remove quote.*certificate/ });
});

test("quotes a native touch selection from the floating bar", async () => {
  serve([reply]);
  const send = sent();
  vi.spyOn(Range.prototype, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 10, y: 10, width: 50 }),
  );
  render(BotChatPage, { botId: bot.bot_id });
  const word = await screen.findByText("certificate");
  await fireEvent.pointerDown(word, { pointerType: "touch" });
  const range = document.createRange();
  range.selectNodeContents(word);
  document.getSelection()!.removeAllRanges();
  document.getSelection()!.addRange(range);
  await fireEvent.pointerCancel(word, { pointerType: "touch" });
  const bar = within(await screen.findByRole("toolbar"));
  expect(bar.queryByRole("button", { name: /Copy/ })).not.toBeInTheDocument();
  await fireEvent.click(bar.getByRole("button", { name: /Quote/ }));
  await userEvent.type(screen.getByRole("textbox"), "Why?");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      expect.stringMatching(
        /^<user_quote message_id="[^"]+" speaker="Bot"[^>]*>certificate<\/user_quote>\nWhy\?$/,
      ),
    ),
  );
});

test.each([false, true])(
  "touch actions quote the selected image from a batch (mosaic: %s)",
  async (mosaic) => {
    const images = Array.from({ length: 7 }, (_, n) =>
      sharedFile(
        `file_0193a000-0000-7000-8000-${String(n + 10).padStart(12, "0")}`,
        `image-${n}.png`,
        "image/png",
      ),
    );
    const share = {
      ...file,
      files: mosaic ? images : [sharedFile(fileId, "checklist.md", "text/markdown"), ...images],
    };
    serve([share]);
    const send = sent();
    render(BotChatPage, { botId: bot.bot_id });
    await fireEvent.click(
      await screen.findByRole("button", { name: mosaic ? /more images/ : /Show .* more/ }),
    );
    const selected = images[6];
    const row = document.querySelector(`[data-bot-file-id="${selected.file_id}"]`)! as HTMLElement;
    await tap(within(row).getByRole("button", { name: "Message actions" }));
    const sheet = within(await screen.findByRole("dialog"));
    await fireEvent.click(sheet.getByRole("menuitem", { name: /Quote image/ }));
    await userEvent.type(screen.getByRole("textbox"), "This one");
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        bot.bot_id,
        `<user_quote file_id="${selected.file_id}" speaker="Bot" path="${selected.path}">${selected.name}</user_quote>\nThis one`,
      ),
    );
  },
);

test.each([false, true])(
  "previews and sends the same capped quote (restored draft: %s)",
  async (restore) => {
    const original = "中".repeat(1999) + "😀x";
    const expected = "中".repeat(1999) + "😀…";
    const entries: BotEntry[] = [message(1, "bot_reply", original)];
    serve(entries);
    const send = sent().mockImplementation(async (_bot, content) => {
      const result = message(99, "user_input", content);
      entries.push(result);
      return result;
    });
    if (restore) {
      localStorage.setItem(
        `pontia:bot-quote:v1:${JSON.stringify([testDevice.id, bot.bot_id])}`,
        JSON.stringify({
          kind: "message",
          messageId: id(1),
          speaker: "Bot",
          text: original,
          excerpt: false,
        }),
      );
    }
    render(BotChatPage, { botId: bot.bot_id });
    await screen.findByText(original);
    if (!restore) await quoteFromMenu(rows()[0]);
    const pending = document.querySelector("[data-bot-pending-quote]")!;
    expect(pending.querySelector("span")!.textContent).toBe(expected);
    await userEvent.type(screen.getByRole("textbox"), "Explain");
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        bot.bot_id,
        `<user_quote message_id="${id(1)}" speaker="Bot" excerpt="true">${expected}</user_quote>\nExplain`,
      ),
    );
    const sentQuote = await screen.findByRole("button", {
      name: `Quote from Helper Bot: ${expected}`,
    });
    expect(sentQuote.querySelector("span")!.textContent).toBe(expected);
  },
);

test("opens context from the message menu and reveals an earlier linked message", async () => {
  const first = { ...reply, content: "Renew the [certificate](/notes.md) first." };
  const latest = message(5, "user_input", "Proceed");
  const pages = serve([latest], { older: [first] });
  const context = vi.spyOn(testDevice.api, "getBotMessageContext").mockResolvedValue({
    messages: [first, latest],
    truncated: false,
  });
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.contextMenu(await screen.findByText("Proceed"), { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "View context" }));
  const panel = await screen.findByRole("complementary");
  const linked = await within(panel).findByRole("button", {
    name: /Renew the certificate first\./,
  });
  expect(context).toHaveBeenCalledWith(bot.bot_id, latest.id, expect.any(AbortSignal));
  await fireEvent.click(linked);
  await waitFor(() =>
    expect(document.querySelector(`[data-bot-entry-id="${first.id}"]`)).toHaveAttribute(
      "data-highlighted",
    ),
  );
  expect(pages.mock.calls.some(([, page]) => page.before === "older")).toBe(true);
});

test("opens an outstanding todo from the feed and quotes it into the composer", async () => {
  const todo = { ...reply, todo: { state: "open" as const } };
  serve([todo]);
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({
    open: [todo],
    recently_resolved: [],
  });
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Todo" }));
  await fireEvent.click(await screen.findByRole("button", { name: "Quote reply" }));
  const textbox = screen.getByRole("textbox");
  await waitFor(() => expect(textbox).toHaveFocus());
  await userEvent.type(textbox, "Approved");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote message_id="${reply.id}" speaker="Bot">${reply.content}</user_quote>\nApproved`,
    ),
  );
});
