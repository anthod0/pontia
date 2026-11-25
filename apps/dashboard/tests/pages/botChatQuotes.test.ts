import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEventBase from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotEntry, BotMessage } from "../../src/api/types";
import { ApiError } from "../../src/api/errors";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import { saveUploadRecord } from "../../src/lib/uploads/records";
import { VisibleObserver, mockFileInfo } from "../components/fileCardFixture";

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
  reasoning_effort: null,
};
const id = (n: number) => `bmsg_0193a000-0000-7000-8000-${String(n).padStart(12, "0")}`;
const message = (n: number, kind: BotMessage["kind"], content: string): BotMessage => ({
  id: id(n),
  kind,
  content,
  status: "normal",
  created_at: new Date(n * 1000).toISOString(),
});
const reply = message(1, "bot_reply", "Renew the **certificate** first.");
/** A Bot reply with an attachments block. */
const attached = (n: number, ...names: string[]) =>
  message(
    n,
    "bot_reply",
    ["Files:", "", "```attachments", ...names.map((name) => `/work/${name}`), "```"].join("\n"),
  );
const file = attached(2, "checklist.md", "chart.png");

function serve(entries: BotEntry[], options: { older?: BotEntry[]; missing?: string[] } = {}) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      if (around !== undefined && options.missing?.includes(around))
        throw new ApiError("Bot timeline entry not found", "not_found", 404);
      const base = {
        bot,
        status: { kind: "idle" as const },
        file_link_session: { session_id: "s", workspace: "/work" },
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

// jsdom cannot hit-test the editable line beside a non-editable widget.
async function typeReply(text: string) {
  const editor = screen.getByRole("textbox");
  editor.focus();
  const paragraph = editor.querySelector("p")!;
  document.getSelection()!.collapse(paragraph, paragraph.childNodes.length);
  await userEvent.keyboard(text);
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
  toast.success.mockReset();
});

test("sends a quoted Bot reply as a user_quote element before the typed text", async () => {
  serve([reply]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  await typeReply("Who owns this?");
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

test("quotes one attached file with its message ID and path", async () => {
  serve([file]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText(/checklist/);
  // The reply text and each attached file are quotable.
  expect(rows()).toHaveLength(3);
  await quoteFromMenu(rows()[1]);
  await typeReply("Check it");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote message_id="${file.id}" speaker="Bot" path="/work/checklist.md">checklist.md</user_quote>\nCheck it`,
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
  await typeReply("ok");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0][1]).toBe(
    `<user_quote message_id="${id(2)}" speaker="Bot">Second answer</user_quote>\nok`,
  );
});

test.each(["Backspace", "deleteContentBackward"])(
  "removes the quote at the start with %s",
  async (key) => {
    serve([reply]);
    const send = sent();
    render(BotChatPage, { botId: bot.bot_id });
    await screen.findByText("certificate");
    await quoteFromMenu(rows()[0]);
    const editor = screen.getByRole("textbox");
    expect(within(editor).getByRole("button", { name: /^Remove quote/ })).toBeInTheDocument();
    await typeReply("Unquoted");
    document.getSelection()!.collapse(editor.querySelector("p"), 0);
    if (key === "Backspace") await fireEvent.keyDown(editor, { key });
    else
      await fireEvent(
        editor,
        new InputEvent("beforeinput", { bubbles: true, cancelable: true, inputType: key }),
      );
    await waitFor(() => expect(within(editor).queryByRole("button")).toBeNull());
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(send).toHaveBeenCalledWith(bot.bot_id, "Unquoted"));
  },
);

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
  await typeReply("x");
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
  await typeReply("hi");
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
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  mockFileInfo();
  vi.spyOn(testDevice.previews, "imageUrl").mockResolvedValue("blob:chart");
  const quoted = message(
    3,
    "user_input",
    `<user_quote message_id="${file.id}" speaker="Bot" path="/work/chart.png">chart.png</user_quote>\nBigger`,
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
  // A mixed card is a list; the quoted row is ringed.
  await waitFor(() =>
    expect(document.querySelector('[data-shared-file="/work/chart.png"]')!.className).toContain(
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
  const replyMenu = screen.getAllByRole("button", { name: "Message actions" })[0];
  const fileRow = document.querySelector<HTMLElement>('[data-bot-file-path="/work/checklist.md"]')!;
  await tap(within(fileRow).getByRole("button", { name: "Message actions" }));
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
  expect(within(menu).getByRole("menuitem", { name: "Mark as todo" })).toBeVisible();
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
  await typeReply("Why?");
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
    vi.stubGlobal("IntersectionObserver", VisibleObserver);
    mockFileInfo();
    const images = Array.from({ length: 7 }, (_, n) => `image-${n}.png`);
    const share = attached(2, ...(mosaic ? images : ["checklist.md", ...images]));
    serve([share]);
    const send = sent();
    render(BotChatPage, { botId: bot.bot_id });
    await fireEvent.click(
      await screen.findByRole("button", { name: mosaic ? /more images/ : /Show .* more/ }),
    );
    const selected = images[6];
    const row = document.querySelector(`[data-bot-file-path="/work/${selected}"]`)! as HTMLElement;
    await tap(within(row).getByRole("button", { name: "Message actions" }));
    const sheet = within(await screen.findByRole("dialog"));
    await fireEvent.click(sheet.getByRole("menuitem", { name: /Quote image/ }));
    await typeReply("This one");
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        bot.bot_id,
        `<user_quote message_id="${share.id}" speaker="Bot" path="/work/${selected}">${selected}</user_quote>\nThis one`,
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
        `pontia:bot-quote:v2:${JSON.stringify([testDevice.id, bot.bot_id])}`,
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
    await typeReply("Explain");
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
  expect(linked.querySelector("time")).toHaveAttribute("datetime", first.created_at);
  await fireEvent.click(linked);
  await waitFor(() =>
    expect(document.querySelector(`[data-bot-entry-id="${first.id}"]`)).toHaveAttribute(
      "data-highlighted",
    ),
  );
  expect(pages.mock.calls.some(([, page]) => page.before === "older")).toBe(true);
});

test("shows whole messages with their line breaks in the context tab", async () => {
  const ending = "This final sentence must stay visible.";
  const first = { ...reply, content: `Checked:\n\n- ${"long ".repeat(500)}\n\n${ending}` };
  const latest = message(5, "user_input", "Proceed");
  serve([first, latest]);
  vi.spyOn(testDevice.api, "getBotMessageContext").mockResolvedValue({
    messages: [first, latest],
    truncated: false,
  });
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.contextMenu(await screen.findByText("Proceed"), { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "View context" }));
  const panel = await screen.findByRole("complementary");
  const text = (await within(panel).findByText(new RegExp(ending))).closest("span")!;
  expect(text.textContent).toBe(`Checked:\n${"long ".repeat(500).trim()}\n${ending}`);
  expect(text).toHaveClass("whitespace-pre-line");
  expect(text).not.toHaveClass("line-clamp-3");
});

const todo = {
  ...reply,
  todo: { state: "open" as const, by: "bot" as const, at: "2026-11-21T08:00:00Z" },
};
const quotedTodo = (text: string) =>
  `<user_quote message_id="${reply.id}" speaker="Bot">${reply.content}</user_quote>\n${text}`;

async function openTodos() {
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({
    open: [todo],
    recently_resolved: [],
  });
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Todo" }));
  const item = (await within(screen.getByRole("complementary")).findAllByRole("listitem"))[0];
  return within(item);
}

test("lists a todo's actions with context first, then Ignore, Quote and Reply", async () => {
  serve([todo]);
  const item = await openTodos();
  const actions = ["Context", "Cancel todo", "Quote", "Reply"].map((name) =>
    item.getByRole("button", { name }),
  );
  for (const [before, after] of actions.slice(0, -1).map((button, i) => [button, actions[i + 1]]))
    expect(before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test("dates a todo and its closing time in the todo panel", async () => {
  const at = "2026-11-21T08:00:00.000Z";
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({
    open: [todo],
    recently_resolved: [{ ...reply, id: id(3), todo: { state: "done", by: "bot", at } }],
  });
  serve([todo]);
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Todo" }));
  const panel = within(screen.getByRole("complementary"));
  const [open] = await panel.findAllByRole("listitem");
  expect(open.querySelector("time")).toHaveAttribute("datetime", todo.created_at);
  await fireEvent.click(panel.getByRole("tab", { name: "Resolved 1" }));
  const done = await panel.findByRole("listitem");
  expect(done.querySelector("time")).toHaveAttribute("datetime", at);
  expect(panel.getByRole("heading", { name: /Nov 21/ })).toBeVisible();
  const mark = vi.spyOn(testDevice.api, "setBotTodo").mockResolvedValue(todo);
  await fireEvent.click(panel.getByRole("button", { name: "Reopen" }));
  await waitFor(() => expect(mark).toHaveBeenCalledWith(bot.bot_id, id(3), true));
});

test("ignoring a todo resolves it like marking it done", async () => {
  serve([todo]);
  const change = vi.spyOn(testDevice.api, "setBotTodo").mockResolvedValue({
    ...todo,
    todo: { state: "done", by: "user", at: new Date().toISOString() },
  });
  const item = await openTodos();
  await fireEvent.click(item.getByRole("button", { name: "Cancel todo" }));
  await waitFor(() => expect(change).toHaveBeenCalledWith(bot.bot_id, todo.id, false));
  await waitFor(() =>
    expect(toast.success).toHaveBeenCalledWith(
      "Todo cancelled",
      expect.objectContaining({ action: expect.objectContaining({ label: "Undo" }) }),
    ),
  );
});

test("ignores an open todo from its message menu", async () => {
  serve([todo]);
  const change = vi.spyOn(testDevice.api, "setBotTodo").mockResolvedValue({
    ...todo,
    todo: { state: "done", by: "user", at: new Date().toISOString() },
  });
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.contextMenu(await screen.findByText("certificate"), { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Cancel todo" }));
  await waitFor(() => expect(change).toHaveBeenCalledWith(bot.bot_id, todo.id, false));
  await waitFor(() =>
    expect(toast.success).toHaveBeenCalledWith(
      "Todo cancelled",
      expect.objectContaining({ action: expect.objectContaining({ label: "Undo" }) }),
    ),
  );
});

test("opens an outstanding todo from the feed and quotes it into the composer", async () => {
  serve([todo]);
  const send = sent();
  const item = await openTodos();
  await fireEvent.click(item.getByRole("button", { name: "Quote" }));
  const textbox = screen.getByRole("textbox");
  await waitFor(() => expect(textbox).toHaveFocus());
  await typeReply("Approved");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(send).toHaveBeenCalledWith(bot.bot_id, quotedTodo("Approved")));
});

test("replies to a todo from its own box with the same request as quoting it, leaving the composer alone", async () => {
  serve([todo, message(2, "bot_reply", "Other answer")]);
  const send = sent();
  const item = await openTodos();
  await quoteFromMenu(rows()[1]);
  const composer = () =>
    within(screen.getByRole("region", { name: bot.name })).getByRole("textbox");
  await typeReply("Composer draft");

  await fireEvent.click(item.getByRole("button", { name: "Reply" }));
  const box = item.getByRole("textbox", { name: "Message" });
  await waitFor(() => expect(box).toHaveFocus());
  await userEvent.type(box, "Approved{Enter}");
  await waitFor(() => expect(send).toHaveBeenCalledWith(bot.bot_id, quotedTodo("Approved")));
  await waitFor(() => expect(item.queryByRole("textbox")).not.toBeInTheDocument());

  expect(composer()).toHaveTextContent("Composer draft");
  expect(screen.getByRole("button", { name: /^Remove quote: Other answer/ })).toBeInTheDocument();
});

test("keeps the reply text after a failed send, retries it from the toast, and closes with Escape", async () => {
  serve([todo]);
  let fail!: (error: Error) => void;
  const send = vi
    .spyOn(testDevice.api, "sendBotMessage")
    .mockImplementationOnce(() => new Promise((_resolve, reject) => (fail = reject)))
    .mockRejectedValueOnce(new Error("still offline"));
  const item = await openTodos();
  await fireEvent.click(item.getByRole("button", { name: "Reply" }));
  const box = item.getByRole("textbox", { name: "Message" });
  await userEvent.type(box, "Approved");
  await fireEvent.click(item.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(box).toHaveAttribute("readonly"));
  expect(item.getByRole("button", { name: "Cancel" })).toBeDisabled();
  await userEvent.keyboard("{Escape}");
  expect(box).toBeInTheDocument();

  fail(new Error("offline"));
  await waitFor(() => expect(toast.error).toHaveBeenCalledWith("offline", expect.anything()));
  expect(box).toHaveValue("Approved");
  expect(box).not.toHaveAttribute("readonly");

  const retry = toast.error.mock.calls[0][1].action.onClick;
  retry(new MouseEvent("click"));
  await waitFor(() => expect(send).toHaveBeenCalledTimes(2));
  expect(send.mock.calls.map(([, content]) => content)).toEqual([
    quotedTodo("Approved"),
    quotedTodo("Approved"),
  ]);
  await waitFor(() => expect(box).not.toHaveAttribute("readonly"));

  box.focus();
  await userEvent.keyboard("{Escape}");
  expect(item.queryByRole("textbox")).not.toBeInTheDocument();
  expect(item.getByRole("button", { name: "Reply" })).toHaveFocus();
});

test("cancels a todo reply without sending it", async () => {
  serve([todo]);
  const send = sent();
  const item = await openTodos();
  const button = item.getByRole("button", { name: "Reply" });
  await fireEvent.click(button);
  expect(button).toHaveAttribute("aria-expanded", "true");
  await userEvent.type(item.getByRole("textbox", { name: "Message" }), "Not yet");
  await fireEvent.click(item.getByRole("button", { name: "Cancel" }));
  expect(item.queryByRole("textbox")).not.toBeInTheDocument();
  expect(button).toHaveAttribute("aria-expanded", "false");
  expect(send).not.toHaveBeenCalled();
});

test("a reply with only attachments keeps its message menu", async () => {
  const only = message(2, "bot_reply", "```attachments\n/work/report.pdf\n```");
  serve([only]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("button", { name: "Download report.pdf" });
  const buttons = screen.getAllByRole("button", { name: "Message actions" });
  await userEvent.click(buttons.at(-1)!);
  const menu = await screen.findByRole("menu");
  const mark = vi
    .spyOn(testDevice.api, "setBotTodo")
    .mockResolvedValue({ ...only, todo: todo.todo });
  await fireEvent.click(within(menu).getByRole("menuitem", { name: "Mark as todo" }));
  await waitFor(() => expect(mark).toHaveBeenCalledWith(bot.bot_id, only.id, true));
});

test("a file's message menu button looks the same as its download button", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  mockFileInfo();
  vi.spyOn(testDevice.previews, "imageUrl").mockImplementation(
    async (_session, path) => `blob:${path}`,
  );
  serve([attached(2, "report.pdf"), attached(3, "a.txt", "b.txt"), attached(4, "x.png", "y.png")]);
  render(BotChatPage, { botId: bot.bot_id });
  const actions = async (name: string) => {
    const download = await screen.findByRole("button", { name: `Download ${name}` });
    const file = download.closest<HTMLElement>("[data-bot-file-path]")!;
    return { download, menu: within(file).getByRole("button", { name: "Message actions" }) };
  };
  for (const name of ["report.pdf", "a.txt", "x.png"]) {
    const { download, menu } = await actions(name);
    expect(menu.className).toBe(download.className);
  }
  // Rows always show both actions; an image tile reveals its menu on hover.
  for (const name of ["report.pdf", "a.txt"])
    expect((await actions(name)).menu.closest(".opacity-0")).toBeNull();
  expect((await actions("x.png")).menu.closest(".opacity-0")).not.toBeNull();
  vi.unstubAllGlobals();
});

test("a block in the middle of a reply shows its card between two bubbles of one message", async () => {
  const middle = message(
    2,
    "bot_reply",
    "Before the file.\n\n```attachments\n/work/report.pdf\n```\n\nAfter the file.",
  );
  serve([middle]);
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByRole("button", { name: "Download report.pdf" });
  const entry = document.querySelector<HTMLElement>(`[data-bot-entry-id="${id(2)}"]`)!;
  expect(
    [...entry.querySelectorAll<HTMLElement>("[data-bot-quotable]")].map(
      (element) => element.dataset.botFilePath ?? element.textContent?.trim(),
    ),
  ).toEqual(["Before the file.", "/work/report.pdf", "After the file."]);
  // Either bubble acts on the whole message.
  await fireEvent.contextMenu(screen.getByText("After the file."), { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Copy" }));
  await waitFor(() =>
    expect(writeText).toHaveBeenCalledWith("Before the file.\n\nAfter the file."),
  );
  await quoteFromMenu(screen.getByText("Before the file."));
  await screen.findByRole("button", { name: /^Remove quote.*Before the file/ });
});
